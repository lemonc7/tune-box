package database

import (
	"context"
	"database/sql"
	"errors"

	"github.com/lemonc7/tune-go/repo"
)

// Store 封装 SQLite 和 sqlc 生成的查询，向业务层提供稳定的数据接口。
type Store struct {
	db      *sql.DB
	queries *repo.Queries
}

// CreateDownloadParams 是创建歌曲和下载任务所需的数据。
type CreateDownloadParams struct {
	Provider   string
	TrackID    string
	Quality    string
	Name       string
	FileName   string
	Artists    []string
	Album      string
	DurationMS int64
	CoverURL   string
}

// LibraryFileParams 描述一次目录扫描发现的音频文件。
type LibraryFileParams struct {
	MusicID   *int64
	Path      string
	FileName  string
	Extension string
	Size      int64
	Modified  string
}

// Close 关闭底层数据库连接池。
func (s *Store) Close() error {
	return s.db.Close()
}

// CreateDownload 在一个事务中保存歌曲并创建任务。同一平台、歌曲和音质只保留一条任务。
func (s *Store) CreateDownload(ctx context.Context, input CreateDownloadParams) (repo.Download, bool, error) {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return repo.Download{}, false, err
	}
	defer tx.Rollback()
	q := s.queries.WithTx(tx)
	// 歌曲、歌手关系和下载记录必须在同一个事务中写入。
	music, err := q.UpsertMusic(ctx, repo.UpsertMusicParams{
		Provider: input.Provider,
		TrackID:  input.TrackID,
		Name:     input.Name, Album: optionalString(input.Album),
		DurationMs: optionalInt64(input.DurationMS), CoverUrl: optionalString(input.CoverURL),
	})
	if err != nil {
		return repo.Download{}, false, err
	}
	for _, artistName := range input.Artists {
		if artistName == "" {
			continue
		}
		artist, err := q.UpsertArtist(ctx, artistName)
		if err != nil {
			return repo.Download{}, false, err
		}
		if err := q.LinkMusicArtist(ctx, repo.LinkMusicArtistParams{MusicID: music.ID, ArtistID: artist.ID}); err != nil {
			return repo.Download{}, false, err
		}
	}
	row, err := q.CreateDownload(ctx, repo.CreateDownloadParams{
		MusicID:  music.ID,
		Provider: input.Provider,
		TrackID:  input.TrackID,
		Quality:  input.Quality,
		FileName: optionalString(input.FileName),
	})
	created := true
	if errors.Is(err, sql.ErrNoRows) {
		created = false
		row, err = q.GetDownloadByKey(ctx, repo.GetDownloadByKeyParams{
			Provider: input.Provider, TrackID: input.TrackID, Quality: input.Quality,
		})
	}
	if err != nil {
		return repo.Download{}, false, err
	}
	if err := tx.Commit(); err != nil {
		return repo.Download{}, false, err
	}
	return row, created, nil
}

// ResetInterruptedDownloads 将上次进程退出时中断的任务放回队列。
func (s *Store) ResetInterruptedDownloads(ctx context.Context, message string) error {
	return s.queries.ResetInterruptedDownloads(ctx, optionalString(message))
}

func (s *Store) ListPendingDownloadIDs(ctx context.Context) ([]int64, error) {
	return s.queries.ListPendingDownloadIDs(ctx)
}

// ClaimDownload 原子地把 pending 任务变为 resolving，避免多个 worker 重复执行。
func (s *Store) ClaimDownload(ctx context.Context, id int64) (repo.Download, error) {
	return s.queries.ClaimDownload(ctx, id)
}

func (s *Store) GetDownload(ctx context.Context, id int64) (repo.Download, error) {
	return s.queries.GetDownload(ctx, id)
}

func (s *Store) GetDownloadJob(ctx context.Context, id int64) (repo.GetDownloadJobRow, error) {
	return s.queries.GetDownloadJob(ctx, id)
}

// MarkDownloadDownloading 保存临时资源地址并将状态更新为正在下载。
func (s *Store) MarkDownloadDownloading(ctx context.Context, id int64, resourceURL string) (bool, error) {
	rows, err := s.queries.MarkDownloadDownloading(ctx, repo.MarkDownloadDownloadingParams{
		ID: id, ResourceUrl: optionalString(resourceURL),
	})
	return rows > 0, err
}

// MarkDownloadCompleted 保存最终文件信息并结束下载记录。
func (s *Store) MarkDownloadCompleted(ctx context.Context, id int64, fileName, path string, size int64, contentType string) (bool, error) {
	rows, err := s.queries.MarkDownloadCompleted(ctx, repo.MarkDownloadCompletedParams{
		ID: id, FileName: optionalString(fileName), Path: optionalString(path),
		Size: &size, ContentType: optionalString(contentType), DownloadedBytes: size, TotalBytes: size,
	})
	return rows > 0, err
}

func (s *Store) UpdateDownloadProgress(ctx context.Context, id, downloaded, total int64) error {
	progress := int64(0)
	if total > 0 {
		progress = downloaded * 100 / total
		if progress > 99 {
			progress = 99
		}
	}
	return s.queries.UpdateDownloadProgress(ctx, repo.UpdateDownloadProgressParams{
		ID: id, Progress: progress, DownloadedBytes: downloaded, TotalBytes: total,
	})
}

// MarkDownloadTagging 表示音频已下载完整，正在内嵌歌词、封面和歌曲标签。
func (s *Store) MarkDownloadTagging(ctx context.Context, id int64) (bool, error) {
	rows, err := s.queries.MarkDownloadTagging(ctx, id)
	return rows > 0, err
}

// MarkDownloadFailed 保存失败原因并结束下载记录。
func (s *Store) MarkDownloadFailed(ctx context.Context, id int64, message string) error {
	return s.queries.MarkDownloadFailed(ctx, repo.MarkDownloadFailedParams{
		ID: id, ErrorMessage: optionalString(message),
	})
}

func (s *Store) PauseDownload(ctx context.Context, id int64) (bool, error) {
	rows, err := s.queries.PauseDownload(ctx, id)
	return rows > 0, err
}

func (s *Store) CancelDownload(ctx context.Context, id int64) (bool, error) {
	rows, err := s.queries.CancelDownload(ctx, repo.CancelDownloadParams{ID: id, ErrorMessage: optionalString("任务已取消")})
	return rows > 0, err
}

func (s *Store) ResumeDownload(ctx context.Context, id int64) (bool, error) {
	rows, err := s.queries.ResumeDownload(ctx, id)
	return rows > 0, err
}

func (s *Store) RetryDownload(ctx context.Context, id int64) (bool, error) {
	rows, err := s.queries.RetryDownload(ctx, id)
	return rows > 0, err
}

func (s *Store) DeleteDownload(ctx context.Context, id int64) (bool, error) {
	rows, err := s.queries.DeleteDownload(ctx, id)
	return rows > 0, err
}

// ListDownloads 按创建顺序倒序返回下载历史。
func (s *Store) ListDownloads(ctx context.Context, limit int64) ([]repo.ListDownloadsRow, error) {
	return s.queries.ListDownloads(ctx, limit)
}

// ListLibraryFiles 返回当前下载目录中的文件记录。
func (s *Store) ListLibraryFiles(ctx context.Context, limit int64) ([]repo.ListLibraryFilesRow, error) {
	return s.queries.ListLibraryFiles(ctx, limit)
}

// SyncLibrary 使用一次扫描结果更新文件表，并清理已经不存在的记录。
func (s *Store) SyncLibrary(ctx context.Context, scanToken string, files []LibraryFileParams) error {
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	q := s.queries.WithTx(tx)
	for _, file := range files {
		if err := q.UpsertLibraryFile(ctx, repo.UpsertLibraryFileParams{
			MusicID: file.MusicID, Path: file.Path, FileName: file.FileName,
			Extension: file.Extension, Size: file.Size, ModifiedAt: file.Modified,
			ScanToken: scanToken,
		}); err != nil {
			return err
		}
	}
	if err := q.DeleteLibraryFilesNotSeen(ctx, scanToken); err != nil {
		return err
	}
	return tx.Commit()
}

// UpsertDownloadedFile 在下载完成后立即登记落盘文件。
func (s *Store) UpsertDownloadedFile(ctx context.Context, musicID int64, scanToken string, file LibraryFileParams) error {
	file.MusicID = &musicID
	return s.queries.UpsertLibraryFile(ctx, repo.UpsertLibraryFileParams{
		MusicID: file.MusicID, Path: file.Path, FileName: file.FileName,
		Extension: file.Extension, Size: file.Size, ModifiedAt: file.Modified,
		ScanToken: scanToken,
	})
}

func optionalString(value string) *string {
	if value == "" {
		return nil
	}
	return &value
}

func optionalInt64(value int64) *int64 {
	if value == 0 {
		return nil
	}
	return &value
}
