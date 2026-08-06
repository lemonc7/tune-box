package service

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"io/fs"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/lemonc7/tune-go/database"
	"github.com/lemonc7/tune-go/download"
	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/provider"
	"github.com/lemonc7/tune-go/tag"
)

var ErrDownloadState = errors.New("当前任务状态不支持该操作")

// Music 统一 REST、MCP 共用的搜索、解析、下载和历史记录逻辑。
type Music struct {
	registry    *provider.Registry
	downloader  *download.Downloader
	embedder    *tag.Embedder
	store       *database.Store
	downloadDir string
	logger      *slog.Logger
	queue       chan int64 // 等待worker处理的任务队列
	workers     int        // 同时运行的下载worker数量
	ctx         context.Context
	stop        context.CancelFunc
	wait        sync.WaitGroup               // 等待所有worker安全退出
	runningMu   sync.Mutex                   // 保护running map
	running     map[int64]context.CancelFunc // 保存只在执行任务的取消函数
}

// NewMusic 创建供 REST 和 MCP 共用的音乐业务服务。
func NewMusic(
	registry *provider.Registry,
	downloader *download.Downloader,
	embedder *tag.Embedder,
	store *database.Store,
	downloadDir string,
	workers int,
	logger *slog.Logger,
) *Music {
	return &Music{
		registry:    registry,
		downloader:  downloader,
		embedder:    embedder,
		store:       store,
		downloadDir: downloadDir,
		workers:     workers,
		logger:      logger,
		queue:       make(chan int64, 1024),
		running:     make(map[int64]context.CancelFunc),
	}
}

// Start 恢复上次中断的任务，并启动固定数量的下载 worker。
func (s *Music) Start(ctx context.Context) error {
	if err := s.store.ResetInterruptedDownloads(ctx, "服务重启，任务已重新入队"); err != nil {
		return err
	}
	ids, err := s.store.ListPendingDownloadIDs(ctx)
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

// Download 创建异步任务并立即返回，真正的解析和下载由 worker 执行。
func (s *Music) Download(ctx context.Context, req model.DownloadRequest) (model.DownloadTaskResult, error) {
	name := strings.TrimSpace(req.Name)
	if name == "" {
		name = strings.TrimSpace(req.FileName)
	}
	if name == "" {
		name = req.Provider + "-" + req.TrackID
	}
	row, created, err := s.store.CreateDownload(ctx, database.CreateDownloadParams{
		Provider:   req.Provider,
		TrackID:    req.TrackID,
		Quality:    req.Quality,
		Name:       name,
		FileName:   req.FileName,
		Artists:    req.Artists,
		Album:      req.Album,
		DurationMS: req.DurationMS,
		CoverURL:   req.CoverURL,
	})
	if err != nil {
		return model.DownloadTaskResult{}, fmt.Errorf("create download task: %w", err)
	}
	if created {
		select {
		case s.queue <- row.ID:
		case <-ctx.Done():
			return model.DownloadTaskResult{}, ctx.Err()
		case <-s.ctx.Done():
			return model.DownloadTaskResult{}, errors.New("download service is stopped")
		}
	}
	return model.DownloadTaskResult{DownloadID: row.ID, Status: row.Status, Duplicate: !created}, nil
}

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
	row, err := s.store.ClaimDownload(s.ctx, id)
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

	job, err := s.store.GetDownloadJob(taskCtx, id)
	if err != nil {
		s.failDownload(id, err)
		return
	}
	resource, err := s.Resolve(taskCtx, model.ResolveRequest{Provider: row.Provider, TrackID: row.TrackID, Quality: row.Quality})
	if err != nil {
		if s.ctx.Err() == nil {
			s.failDownload(id, err)
		}
		return
	}
	downloading, err := s.store.MarkDownloadDownloading(taskCtx, id, resource.URL)
	if err != nil {
		if s.ctx.Err() == nil {
			s.failDownload(id, err)
		}
		return
	}
	if !downloading {
		return
	}
	fileName := job.MusicName
	if job.FileName != nil && *job.FileName != "" {
		fileName = *job.FileName
	}
	lastUpdate := time.Time{}
	result, err := s.downloader.Download(taskCtx, resource, download.Options{
		TaskID: id, FileName: fileName, ExpectedBytes: job.TotalBytes,
		Progress: func(downloaded, total int64) {
			if time.Since(lastUpdate) < 300*time.Millisecond && (total == 0 || downloaded < total) {
				return
			}
			lastUpdate = time.Now()
			progressCtx, progressCancel := context.WithTimeout(context.Background(), time.Second)
			if updateErr := s.store.UpdateDownloadProgress(progressCtx, id, downloaded, total); updateErr != nil {
				s.logger.Error("update download progress", "downloadId", id, "error", updateErr)
			}
			progressCancel()
		},
		BeforeFinalize: func(ctx context.Context, path string) error {
			tagging, err := s.store.MarkDownloadTagging(ctx, id)
			if err != nil {
				return err
			}
			if !tagging {
				if err := ctx.Err(); err != nil {
					return err
				}
				return ErrDownloadState
			}
			// 第一阶段仅处理 FLAC，其他音频格式保持原文件内容。
			if resource.Quality.Format != "flac" {
				return nil
			}
			metadata := model.MusicMetadata{Name: job.MusicName}
			if job.MusicAlbum != nil {
				metadata.Album = *job.MusicAlbum
			}
			if job.MusicCoverUrl != nil {
				metadata.CoverURL = *job.MusicCoverUrl
			}
			if job.MusicArtists != "" {
				metadata.Artists = strings.Split(job.MusicArtists, ";")
			}
			item, err := s.registry.Get(job.Provider)
			if err != nil {
				return err
			}
			providerMetadata, metadataErr := item.Metadata(ctx, job.TrackID)
			if metadataErr != nil {
				// 元数据接口短暂缺少某一项时仍写入已经取得的字段，避免浪费完整音频。
				s.logger.Warn("fetch music metadata", "provider", job.Provider, "trackId", job.TrackID, "error", metadataErr)
			}
			if providerMetadata.Name != "" && (metadata.Name == "" || metadata.Name == job.Provider+"-"+job.TrackID) {
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
	completed, err := s.store.MarkDownloadCompleted(finishCtx, id, result.FileName, result.Path, result.Size, result.ContentType)
	finishCancel()
	if err != nil {
		os.Remove(result.Path)
		s.failDownload(id, err)
		return
	}
	if !completed {
		// 下载结束的瞬间若收到暂停或取消，不保留未被任务接受的最终文件。
		os.Remove(result.Path)
		return
	}
	// 下载记录完成后，将实际文件同步进本地曲库表。
	info, statErr := os.Stat(result.Path)
	if statErr == nil {
		file := libraryFile(result.Path, info)
		if err := s.store.UpsertDownloadedFile(taskCtx, row.MusicID, scanToken(), file); err != nil {
			s.logger.Error("record downloaded library file", "path", result.Path, "error", err)
		}
	}
}

// ListDownloads 返回统一格式的下载历史。
func (s *Music) ListDownloads(ctx context.Context, limit int64) ([]model.DownloadHistory, error) {
	rows, err := s.store.ListDownloads(ctx, limit)
	if err != nil {
		return nil, err
	}
	result := make([]model.DownloadHistory, 0, len(rows))
	for _, row := range rows {
		result = append(result, model.DownloadHistory{
			ID: row.ID, MusicID: row.MusicID, MusicName: row.MusicName,
			Album:    row.MusicAlbum,
			Provider: row.Provider, TrackID: row.TrackID, Quality: row.Quality,
			Status: row.Status, Progress: row.Progress, DownloadedBytes: row.DownloadedBytes,
			TotalBytes: row.TotalBytes, Attempt: row.Attempt,
			FileName: row.FileName, Path: row.Path, Size: row.Size,
			ContentType: row.ContentType, ErrorMessage: row.ErrorMessage,
			CreatedAt: row.CreatedAt, StartedAt: row.StartedAt, FinishedAt: row.FinishedAt,
		})
	}
	return result, nil
}

// PauseDownload 暂停排队或正在执行的任务，并保留已下载的断点文件。
func (s *Music) PauseDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	changed, err := s.store.PauseDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if !changed {
		if _, err := s.store.GetDownload(ctx, id); err != nil {
			return model.DownloadTaskResult{}, err
		}
		return model.DownloadTaskResult{}, ErrDownloadState
	}
	s.cancelRunning(id)
	return model.DownloadTaskResult{DownloadID: id, Status: "paused"}, nil
}

// ResumeDownload 将 paused 任务放回队列，下载器会尝试从断点继续。
func (s *Music) ResumeDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	changed, err := s.store.ResumeDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if !changed {
		if _, err := s.store.GetDownload(ctx, id); err != nil {
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
	changed, err := s.store.CancelDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if !changed {
		if _, err := s.store.GetDownload(ctx, id); err != nil {
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
	changed, err := s.store.RetryDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if !changed {
		if _, err := s.store.GetDownload(ctx, id); err != nil {
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
	row, err := s.store.GetDownload(ctx, id)
	if err != nil {
		return err
	}
	if row.Status == "resolving" || row.Status == "downloading" || row.Status == "tagging" {
		if _, err := s.store.CancelDownload(ctx, id); err != nil {
			return err
		}
		s.cancelRunning(id)
	}
	deleted, err := s.store.DeleteDownload(ctx, id)
	if err != nil {
		return err
	}
	if !deleted {
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

// ListLibrary 返回下载目录中的音频文件信息。
func (s *Music) ListLibrary(ctx context.Context, limit int64) ([]model.LibraryFile, error) {
	rows, err := s.store.ListLibraryFiles(ctx, limit)
	if err != nil {
		return nil, err
	}
	result := make([]model.LibraryFile, 0, len(rows))
	for _, row := range rows {
		result = append(result, model.LibraryFile{
			ID:         row.ID,
			MusicID:    row.MusicID,
			Path:       row.Path,
			FileName:   row.FileName,
			Extension:  row.Extension,
			Size:       row.Size,
			ModifiedAt: row.ModifiedAt,
			Provider:   row.Provider,
			TrackID:    row.TrackID,
			MusicName:  row.MusicName,
			Album:      row.MusicAlbum,
		})
	}
	return result, nil
}

// ScanLibrary 扫描下载目录中的音频文件，并让数据库与磁盘保持一致。
func (s *Music) ScanLibrary(ctx context.Context) (int, error) {
	absDir, err := filepath.Abs(s.downloadDir)
	if err != nil {
		return 0, err
	}
	files := make([]database.LibraryFileParams, 0)
	err = filepath.WalkDir(absDir, func(path string, entry fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if entry.IsDir() || !isAudioFile(path) {
			return nil
		}
		info, err := entry.Info()
		if err != nil {
			return err
		}
		files = append(files, libraryFile(path, info))
		return nil
	})
	if err != nil {
		if os.IsNotExist(err) {
			files = nil
		} else {
			return 0, err
		}
	}
	if err := s.store.SyncLibrary(ctx, scanToken(), files); err != nil {
		return 0, err
	}
	return len(files), nil
}

func (s *Music) failDownload(id int64, cause error) {
	// 原请求可能已经超时或取消，使用独立短超时确保失败状态仍有机会落库。
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	if err := s.store.MarkDownloadFailed(ctx, id, cause.Error()); err != nil {
		s.logger.Error("mark download failed", "downloadId", id, "error", err)
	}
}

func libraryFile(path string, info fs.FileInfo) database.LibraryFileParams {
	absPath, _ := filepath.Abs(path)
	return database.LibraryFileParams{
		Path:      absPath,
		FileName:  info.Name(),
		Extension: strings.TrimPrefix(strings.ToLower(filepath.Ext(info.Name())), "."),
		Size:      info.Size(),
		Modified:  info.ModTime().UTC().Format(time.RFC3339Nano),
	}
}

func isAudioFile(path string) bool {
	switch strings.ToLower(filepath.Ext(path)) {
	case ".flac", ".mp3", ".ape", ".wav", ".m4a", ".aac", ".ogg", ".opus":
		return true
	default:
		return false
	}
}

func scanToken() string { return time.Now().UTC().Format(time.RFC3339Nano) }
