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

type Music struct {
	db          *sql.DB
	queries     *repo.Queries
	registry    *provider.Registry
	downloader  *download.Downloader
	embedder    *tag.Embedder
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

func NewMusic(
	db *sql.DB,
	queries *repo.Queries,
	registry *provider.Registry,
	downloader *download.Downloader,
	embedder *tag.Embedder,
	downloadDir string,
	workers int,
	logger *slog.Logger,
) *Music {
	return &Music{
		db:          db,
		queries:     queries,
		registry:    registry,
		downloader:  downloader,
		embedder:    embedder,
		downloadDir: downloadDir,
		workers:     workers,
		logger:      logger,
		queue:       make(chan int64, 1024),
		running:     make(map[int64]context.CancelFunc),
	}
}

func (s *Music) Start(ctx context.Context) error {
	// 应用启动，重新启动未完成的任务
	if err := s.queries.ResetInterruptedDownloads(
		ctx,
		new("服务重启，任务已重新入队"),
	); err != nil {
		return err
	}
	ids, err := s.queries.ListPendingDownloadIDs(ctx)
	if err != nil {
		return err
	}
	s.ctx, s.stop = context.WithCancel(context.Background())

	// 启动worker
	for i := 0; i < s.workers; i++ {
		s.wait.Go(s.worker)
	}
	// 新增任务
	for _, id := range ids {
		s.queue <- id
	}
	return nil
}

func (s *Music) Close() {
	if s.stop != nil {
		s.stop()
		s.wait.Wait()
	}
}

func (s *Music) Providers() []provider.Info { return s.registry.List() }

func (s *Music) Search(ctx context.Context, req model.SearchRequest) (model.SearchResult, error) {
	item, err := s.registry.Get(req.Provider)
	if err != nil {
		return model.SearchResult{}, err
	}
	result, err := item.Search(
		ctx,
		strings.TrimSpace(req.Query),
		req.Page,
		req.Limit,
	)
	if err != nil {
		return model.SearchResult{}, err
	}
	if req.Lossless {
		result = provider.FilterLossless(result)
	}
	return result, nil
}

func (s *Music) Resolve(ctx context.Context, req model.ResolveRequest) (model.Resource, error) {
	item, err := s.registry.Get(req.Provider)
	if err != nil {
		return model.Resource{}, err
	}
	return item.Resolve(ctx, req.TrackID, req.Quality)
}

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
		Name:     name,
		Artists:  string(artists),
		Album:    &req.Album,
		CoverUrl: &req.CoverURL,
	})
	created := true
	if errors.Is(err, sql.ErrNoRows) {
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
	return model.DownloadTaskResult{
		DownloadID: row.ID,
		Status:     row.Status,
		Duplicate:  !created,
	}, nil
}

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
	downloading, err := s.queries.MarkDownloadDownloading(taskCtx, repo.MarkDownloadDownloadingParams{
		ID:          id,
		ResourceUrl: &resource.URL,
	})
	if err != nil {
		if s.ctx.Err() == nil {
			s.failDownload(id, err)
		}
		return
	}
	if downloading == 0 {
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
			tagging, err := s.queries.MarkDownloadTagging(ctx, id)
			if err != nil {
				return err
			}
			if tagging == 0 {
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
				s.logger.Warn("fetch music metadata", "provider", row.Provider, "trackId", row.TrackID, "error", metadataErr)
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
	completed, err := s.queries.MarkDownloadCompleted(finishCtx, repo.MarkDownloadCompletedParams{
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
	if completed == 0 {
		os.Remove(result.Path)
		return
	}
	info, statErr := os.Stat(result.Path)
	if statErr == nil {
		var artistNames []string
		json.Unmarshal([]byte(row.Artists), &artistNames)
		ext := strings.TrimPrefix(strings.ToLower(filepath.Ext(info.Name())), ".")
		if err := s.upsertSong(taskCtx, result.Path, row.Name, ext, artistNames); err != nil {
			s.logger.Error("record downloaded library file", "path", result.Path, "error", err)
		}
	}
}

func (s *Music) upsertSong(ctx context.Context, path string, name string, extension string, artistNames []string) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	q := s.queries.WithTx(tx)

	songID, err := q.UpsertSong(ctx, repo.UpsertSongParams{
		Path:      path,
		Name:      name,
		Extension: extension,
	})
	if err != nil {
		return fmt.Errorf("upsert song: %w", err)
	}

	if err := q.ClearSongArtists(ctx, songID); err != nil {
		return fmt.Errorf("clear song artists: %w", err)
	}

	for _, artistName := range artistNames {
		if artistName == "" {
			continue
		}
		artistID, err := q.UpsertArtist(ctx, artistName)
		if err != nil {
			return fmt.Errorf("upsert artist %q: %w", artistName, err)
		}
		if err := q.AddSongArtist(ctx, repo.AddSongArtistParams{
			SongID:   songID,
			ArtistID: artistID},
		); err != nil {
			return fmt.Errorf("add song artist: %w", err)
		}
	}

	return tx.Commit()
}

func (s *Music) ListDownloads(ctx context.Context, limit, offset int64) ([]model.DownloadHistory, error) {
	rows, err := s.queries.ListDownloads(ctx, repo.ListDownloadsParams{
		Offset: offset,
		Limit:  limit,
	})
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

func (s *Music) PauseDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	changed, err := s.queries.PauseDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if changed == 0 {
		if _, err := s.queries.GetDownload(ctx, id); err != nil {
			return model.DownloadTaskResult{}, err
		}
		return model.DownloadTaskResult{}, ErrDownloadState
	}
	s.cancelRunning(id)
	return model.DownloadTaskResult{DownloadID: id, Status: "paused"}, nil
}

func (s *Music) ResumeDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	changed, err := s.queries.ResumeDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if changed == 0 {
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

func (s *Music) CancelDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	changed, err := s.queries.CancelDownload(ctx, repo.CancelDownloadParams{
		ID:           id,
		ErrorMessage: new("任务已取消")})
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if changed == 0 {
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

func (s *Music) RetryDownload(ctx context.Context, id int64) (model.DownloadTaskResult, error) {
	changed, err := s.queries.RetryDownload(ctx, id)
	if err != nil {
		return model.DownloadTaskResult{}, err
	}
	if changed == 0 {
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
		return model.DownloadTaskResult{}, fmt.Errorf("download service is stopped")
	}
	return model.DownloadTaskResult{DownloadID: id, Status: "pending"}, nil
}

func (s *Music) DeleteDownload(ctx context.Context, id int64) error {
	row, err := s.queries.GetDownload(ctx, id)
	if err != nil {
		return err
	}
	if row.Status == "resolving" || row.Status == "downloading" || row.Status == "tagging" {
		if _, err := s.queries.CancelDownload(ctx, repo.CancelDownloadParams{
			ID:           id,
			ErrorMessage: new("任务已取消")},
		); err != nil {
			return err
		}
		s.cancelRunning(id)
	}
	deleted, err := s.queries.DeleteDownload(ctx, id)
	if err != nil {
		return err
	}
	if deleted == 0 {
		return ErrDownloadState
	}
	return s.downloader.RemovePartial(id)
}

func (s *Music) ListSongs(ctx context.Context, offset, limit int64) ([]repo.ListSongsRow, error) {
	return s.queries.ListSongs(ctx, repo.ListSongsParams{
		Offset: offset,
		Limit:  limit,
	})
}

func (s *Music) SearchSongs(ctx context.Context, keyword string, limit int64) ([]repo.SearchSongsRow, error) {
	return s.queries.SearchSongs(ctx, repo.SearchSongsParams{
		Keyword: &keyword,
		Limit:   limit,
	})
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
		s.logger.Error("mark download failed", "downloadId", id, "error", err)
	}
}
