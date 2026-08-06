package service

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/lemonc7/tune-go/download"
	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/provider"
	"github.com/lemonc7/tune-go/repo"
	"github.com/lemonc7/tune-go/tag"
)

var ErrDownloadState = errors.New("当前任务状态不支持该操作")

// Music 统一 REST、MCP 共用的搜索、解析、下载和历史记录逻辑。
type Music struct {
	registry    *provider.Registry
	downloader  *download.Downloader
	embedder    *tag.Embedder
	queries     repo.Querier
	downloadDir string
	logger      *slog.Logger
	queue       chan int64
	workers     int
	ctx         context.Context
	stop        context.CancelFunc
	wait        sync.WaitGroup
	runningMu   sync.Mutex
	running     map[int64]context.CancelFunc
}

// NewMusic 创建供 REST 和 MCP 共用的音乐业务服务。
func NewMusic(
	registry *provider.Registry,
	downloader *download.Downloader,
	embedder *tag.Embedder,
	queries repo.Querier,
	downloadDir string,
	workers int,
	logger *slog.Logger,
) *Music {
	return &Music{
		registry:    registry,
		downloader:  downloader,
		embedder:    embedder,
		queries:     queries,
		downloadDir: downloadDir,
		workers:     workers,
		logger:      logger,
		queue:       make(chan int64, 1024),
		running:     make(map[int64]context.CancelFunc),
	}
}

// Start 恢复上次中断的任务，并启动固定数量的下载 worker。
func (s *Music) Start(ctx context.Context) error {
	if err := s.queries.ResetInterruptedDownloads(ctx, new("服务重启，任务已重新入队")); err != nil {
		return err
	}

	ids, err := s.queries.ListPendingDownloadIDs(ctx)
	if err != nil {
		return err
	}
	s.ctx, s.stop = context.WithCancel(context.Background())

	for i := 0; i < s.workers; i++ {
		s.wait.Go(s.worker)
	}
	for _, id := range ids {
		s.queue <- id
	}
	return nil
}

// Close 停止 worker。正在下载的任务保留断点，下次启动自动继续。
func (s *Music) Close() {
	if s.stop != nil {
		s.stop()
		s.wait.Wait()
	}
}

// Providers 返回所有已注册音乐源的状态。
func (s *Music) Providers() []provider.Info { return s.registry.List() }

// Search 调用指定音乐源搜索，并按需过滤有损音质。
func (s *Music) Search(ctx context.Context, req model.SearchRequest) (model.SearchResult, error) {
	item, err := s.registry.Get(req.Provider)
	if err != nil {
		return model.SearchResult{}, err
	}
	result, err := item.Search(ctx, strings.TrimSpace(req.Query), req.Page, req.Limit)
	if err != nil {
		return model.SearchResult{}, err
	}
	if req.Lossless {
		result = provider.FilterLossless(result)
	}
	return result, nil
}

// Resolve 调用指定音乐源解析临时下载资源。
func (s *Music) Resolve(ctx context.Context, req model.ResolveRequest) (model.Resource, error) {
	item, err := s.registry.Get(req.Provider)
	if err != nil {
		return model.Resource{}, err
	}
	return item.Resolve(ctx, req.TrackID, req.Quality)
}

// Download 创建异步下载任务。同一平台同一歌曲同音质通过 UNIQUE 约束自动去重。
func (s *Music) Download(ctx context.Context, req model.DownloadRequest) (model.DownloadTaskResult, error) {
	name := strings.TrimSpace(req.Name)
	if name == "" {
		name = strings.TrimSpace(req.FileName)
	}
	if name == "" {
		name = req.Provider + "-" + req.TrackID
	}

	artists, err := json.Marshal(req.Artists)
	if err != nil {
		return model.DownloadTaskResult{}, fmt.Errorf("encode artists: %w", err)
	}
	row, err := s.queries.CreateDownload(ctx, repo.CreateDownloadParams{
		Provider: req.Provider,
		TrackID:  req.TrackID,
		Quality:  req.Quality,
		Name:     req.Name,
		Artists:  string(artists),
		Album:    &req.Album,
		CoverUrl: &req.CoverURL,
	})
	created := true
	if errors.Is(err, sql.ErrNoRows) {
		// 出错或者已存在
		created = false
		row, err = s.queries.GetDownloadByKey(ctx, repo.GetDownloadByKeyParams{
			Provider: req.Provider,
			TrackID:  req.TrackID,
			Quality:  req.Quality,
		})
	}

	if err != nil {
		return model.DownloadTaskResult{}, fmt.Errorf("create download task: %w", err)
	}

	if created {
		if err := s.enqueue(ctx, row.ID); err != nil {
			return model.DownloadTaskResult{}, err
		}
	}
	return model.DownloadTaskResult{DownloadID: row.ID, Status: row.Status, Duplicate: !created}, nil
}

// 将下载任务放入队列，等待worker处理
func (s *Music) enqueue(ctx context.Context, id int64) error {
	select {
	case s.queue <- id:
		return nil
	case <-ctx.Done():
		return ctx.Err()
	case <-s.ctx.Done():
		return errors.New("download service is stopped")
	}
}

// worker下载任务
func (s *Music) worker() {
	for {
		select {
		case <-s.ctx.Done():
			return
		case id := <-s.queue:
			s.runDownload(id)
		}
	}
}

func (s *Music) runDownload(id int64) {
	row, err := s.queries.ClaimDownload(s.ctx, id)
	if errors.Is(err, sql.ErrNoRows) {
		return
	}
	if err != nil {
		s.logger.Error("claim download", "downloadId", id, "error", err)
		return
	}
	taskCtx, cancel := context.WithCancel(s.ctx)
	s.runningMu.Lock()
	s.running[id] = cancel
	s.runningMu.Unlock()
	defer func() {
		cancel()
		s.runningMu.Lock()
		delete(s.running, id)
		s.runningMu.Unlock()
	}()

	resource, err := s.Resolve(taskCtx, model.ResolveRequest{
		Provider: row.Provider,
		TrackID:  row.TrackID,
		Quality:  row.Quality,
	})
	if err != nil {
		if s.ctx.Err() == nil {
			s.failDownload(id, err)
		}
		return
	}
	rows, err := s.queries.MarkDownloadDownloading(taskCtx, repo.MarkDownloadDownloadingParams{
		ID:          id,
		ResourceUrl: &resource.URL,
	})
	if err != nil {
		if s.ctx.Err() == nil {
			s.failDownload(id, err)
		}
		return
	}
	if !(rows > 0) {
		return
	}

	lastUpdate := time.Time{}
	result, err := s.downloader.Download(taskCtx, resource, download.Options{
		TaskID:        id,
		FileName:      row.Name,
		ExpectedBytes: row.TotalBytes,
		Progress: func(downloaded, total int64) {
			if time.Since(lastUpdate) < 300*time.Millisecond && (total == 0 || downloaded < total) {
				return
			}
			lastUpdate = time.Now()
			progressCtx, progressCancel := context.WithTimeout(context.Background(), time.Second)
			if updateErr := s.queries.UpdateDownloadProgress(progressCtx, repo.UpdateDownloadProgressParams{
				ID:              id,
				DownloadedBytes: downloaded,
				TotalBytes:      total,
			}); updateErr != nil {
				s.logger.Error("update download progress", "downloadId", id, "error", updateErr)
			}
			progressCancel()
		},
		BeforeFinalize: func(ctx context.Context, path string) error {
			rows, err := s.queries.MarkDownloadTagging(ctx, id)
			if err != nil {
				return err
			}
			if !(rows > 0) {
				if err := ctx.Err(); err != nil {
					return err
				}
				return ErrDownloadState
			}
			if resource.Quality.Format != "flac" {
				return nil
			}
			metadata := model.MusicMetadata{Name: row.Name}
			if row.Album != nil {
				metadata.Album = *row.Album
			}
			if row.CoverUrl != nil {
				metadata.CoverURL = *row.CoverUrl
			}
			if err := json.Unmarshal([]byte(row.Artists), &metadata.Artists); err != nil {
				return fmt.Errorf("decode download artists: %w", err)
			}
			item, err := s.registry.Get(row.Provider)
			if err != nil {
				return err
			}
			providerMetadata, metadataErr := item.Metadata(ctx, row.TrackID)
			if metadataErr != nil {
				s.logger.Warn(
					"fetch music metadata",
					"provider", row.Provider,
					"trackId", row.TrackID,
					"error", metadataErr)
			}
			if providerMetadata.Name != "" &&
				(metadata.Name == "" || metadata.Name == row.Provider+"-"+row.TrackID) {
				metadata.Name = providerMetadata.Name
			}
			if len(metadata.Artists) == 0 {
				metadata.Artists = providerMetadata.Artists
			}
			if metadata.Album == "" {
				metadata.Album = providerMetadata.Album
			}
			if metadata.CoverURL == "" {
				metadata.CoverURL = providerMetadata.CoverURL
			}
			metadata.Lyrics = providerMetadata.Lyrics
			return s.embedder.EmbedFLAC(ctx, path, metadata)
		},
	})
	if err != nil {
		if s.ctx.Err() == nil {
			s.failDownload(id, err)
		}
		return
	}

	finishCtx, finishCancel := context.WithTimeout(context.Background(), 3*time.Second)
	rs, err := s.queries.MarkDownloadCompleted(finishCtx, repo.MarkDownloadCompletedParams{
		ID:              id,
		Path:            &result.Path,
		Size:            &result.Size,
		DownloadedBytes: result.Size,
		TotalBytes:      result.Size,
	})
	finishCancel()
	if err != nil {
		os.Remove(result.Path)
		s.failDownload(id, err)
		return
	}
	if !(rs > 0) {
		os.Remove(result.Path)
		return
	}

	info, statErr := os.Stat(result.Path)
	if statErr == nil {
		if err := s.queries.UpsertLibraryFile(taskCtx, repo.UpsertLibraryFileParams{
			DownloadID: id,
			Name:       row.Name,
			Artists:    row.Artists,
			FileName:   info.Name(),
			Extension:  strings.TrimPrefix(strings.ToLower(filepath.Ext(info.Name())), "."),
			Size:       info.Size(),
			ModifiedAt: info.ModTime().Format(time.RFC3339Nano),
		}); err != nil {
			s.logger.Error(
				"record downloaded library file",
				"path", result.Path,
				"error", err,
			)
		}
	}
}

// ListDownloads 返回统一格式的下载历史。
func (s *Music) ListDownloads(ctx context.Context, limit int64) ([]model.DownloadHistory, error) {
	rows, err := s.queries.ListDownloads(ctx, limit)
	if err != nil {
		return nil, err
	}
	result := make([]model.DownloadHistory, 0, len(rows))
	for _, row := range rows {
		var artists []string
		if err := json.Unmarshal([]byte(row.Artists), &artists); err != nil {
			return nil, fmt.Errorf("decode download artists: %w", err)
		}
		var progress int64
		if row.TotalBytes > 0 {
			progress = row.DownloadedBytes * 100 / row.TotalBytes
		}
		var fileName *string
		if row.Path != nil {
			fn := filepath.Base(*row.Path)
			fileName = &fn
		}
		result = append(result, model.DownloadHistory{
			ID:              row.ID,
			Name:            row.Name,
			Artists:         artists,
			Album:           row.Album,
			Provider:        row.Provider,
			TrackID:         row.TrackID,
			Quality:         row.Quality,
			Status:          row.Status,
			Progress:        progress,
			DownloadedBytes: row.DownloadedBytes,
			TotalBytes:      row.TotalBytes,
			Attempt:         row.Attempt,
			FileName:        fileName,
			Path:            row.Path,
			Size:            row.Size,
			ErrorMessage:    row.ErrorMessage,
			CreatedAt:       row.CreatedAt,
			StartedAt:       row.StartedAt,
			FinishedAt:      row.FinishedAt,
		})
	}
	return result, nil
}

// PauseDownload 暂停排队或正在执行的任务，并保留已下载的断点文件。
func (s *Music) PauseDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	rows, err := s.queries.PauseDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if !(rows > 0) {
		if _, err := s.queries.GetDownload(ctx, id); err != nil {
			return model.DownloadTaskResult{}, err
		}
		return model.DownloadTaskResult{}, ErrDownloadState
	}
	s.cancelRunning(id)
	return model.DownloadTaskResult{DownloadID: id, Status: "paused"}, nil
}

// ResumeDownload 将 paused 任务放回队列，下载器会尝试从断点继续。
func (s *Music) ResumeDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	rows, err := s.queries.ResumeDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if !(rows > 0) {
		if _, err := s.queries.GetDownload(ctx, id); err != nil {
			return model.DownloadTaskResult{}, err
		}
		return model.DownloadTaskResult{}, ErrDownloadState
	}
	select {
	case s.queue <- id:
	case <-ctx.Done():
		return model.DownloadTaskResult{}, ctx.Err()
	case <-s.ctx.Done():
		return model.DownloadTaskResult{}, errors.New("download service is stopped")
	}
	return model.DownloadTaskResult{DownloadID: id, Status: "pending"}, nil
}

// CancelDownload 终止任务并删除断点文件。
func (s *Music) CancelDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	rows, err := s.queries.CancelDownload(ctx, repo.CancelDownloadParams{
		ID:           id,
		ErrorMessage: new("任务已取消"),
	})
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if !(rows > 0) {
		if _, err := s.queries.GetDownload(ctx, id); err != nil {
			return model.DownloadTaskResult{}, err
		}
		return model.DownloadTaskResult{}, ErrDownloadState
	}
	s.cancelRunning(id)
	if err := s.downloader.RemovePartial(id); err != nil {
		return model.DownloadTaskResult{}, err
	}
	return model.DownloadTaskResult{DownloadID: id, Status: "cancelled"}, nil
}

// RetryDownload 重试 failed 或 cancelled 任务。失败任务若有断点会继续下载。
func (s *Music) RetryDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	rows, err := s.queries.RetryDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if !(rows > 0) {
		if _, err := s.queries.GetDownload(ctx, id); err != nil {
			return model.DownloadTaskResult{}, err
		}
		return model.DownloadTaskResult{}, ErrDownloadState
	}
	select {
	case s.queue <- id:
	case <-ctx.Done():
		return model.DownloadTaskResult{}, ctx.Err()
	case <-s.ctx.Done():
		return model.DownloadTaskResult{}, errors.New("download service is stopped")
	}
	return model.DownloadTaskResult{DownloadID: id, Status: "pending"}, nil
}

// DeleteDownload 删除任务记录和断点文件，已完成的音乐文件不会被删除。
func (s *Music) DeleteDownload(ctx context.Context, id int64) error {
	row, err := s.queries.GetDownload(ctx, id)
	if err != nil {
		return err
	}
	if row.Status == "resolving" || row.Status == "downloading" || row.Status == "tagging" {
		if _, err := s.queries.CancelDownload(ctx, repo.CancelDownloadParams{
			ID:           id,
			ErrorMessage: new("任务已取消"),
		}); err != nil {
			return err
		}
		s.cancelRunning(id)
	}
	rows, err := s.queries.DeleteDownload(ctx, id)
	if err != nil {
		return err
	}
	if !(rows > 0) {
		return ErrDownloadState
	}
	return s.downloader.RemovePartial(id)
}

func (s *Music) cancelRunning(id int64) {
	s.runningMu.Lock()
	cancel := s.running[id]
	s.runningMu.Unlock()
	if cancel != nil {
		cancel()
	}
}

func (s *Music) failDownload(id int64, cause error) {
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	if err := s.queries.MarkDownloadFailed(ctx, repo.MarkDownloadFailedParams{
		ID:           id,
		ErrorMessage: new(cause.Error()),
	}); err != nil {
		s.logger.Error(
			"mark download failed",
			"downloadId", id,
			"error", err,
		)
	}
}
