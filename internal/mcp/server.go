// Package mcp 将 Tune Go 的音乐搜索、下载及历史能力暴露为 MCP 工具。
package mcp

import (
	"context"
	"errors"
	"log/slog"
	"net/http"
	"strings"

	"github.com/lemonc7/tune-go/internal/model"
	"github.com/lemonc7/tune-go/internal/provider"
	"github.com/lemonc7/tune-go/internal/service"
	protocol "github.com/modelcontextprotocol/go-sdk/mcp"
)

// Server 保存 MCP 协议服务和底层音乐业务服务。
type Server struct {
	music  *service.Music
	logger *slog.Logger
	mcp    *protocol.Server
}

type emptyInput struct{}
type providersOutput struct {
	Providers []provider.Info `json:"providers"`
}

type searchInput struct {
	Provider string `json:"provider" jsonschema:"音乐源名称，目前支持 kuwo"`
	Query    string `json:"query" jsonschema:"歌曲名、歌手名或专辑名"`
	Page     int    `json:"page,omitempty" jsonschema:"页码，默认为 1，最大为 10000"`
	Limit    int    `json:"limit,omitempty" jsonschema:"每页数量，默认为 20，最大为 100"`
	Lossless bool   `json:"lossless,omitempty" jsonschema:"是否只返回包含无损音质的歌曲"`
}
type searchOutput struct {
	Result model.SearchResult `json:"result"`
}

type resolveInput struct {
	Provider string `json:"provider" jsonschema:"音乐源名称，目前支持 kuwo"`
	TrackID  string `json:"trackId" jsonschema:"搜索结果中的歌曲 ID"`
	Quality  string `json:"quality" jsonschema:"搜索结果中的音质 ID，例如 2000kflac 或 lossless"`
}
type resolveOutput struct {
	Resource model.Resource `json:"resource"`
}

type downloadInput struct {
	Provider    string   `json:"provider" jsonschema:"音乐源名称，目前支持 kuwo"`
	TrackID     string   `json:"trackId" jsonschema:"搜索结果中的歌曲 ID"`
	Quality     string   `json:"quality" jsonschema:"搜索结果中的音质 ID，例如 2000kflac 或 lossless"`
	FileName    string   `json:"fileName,omitempty" jsonschema:"可选的保存文件名，不需要包含扩展名"`
	Name        string   `json:"name,omitempty" jsonschema:"搜索结果中的歌曲名，用于保存曲库信息"`
	Artists     []string `json:"artists,omitempty" jsonschema:"搜索结果中的歌手列表，用于保存曲库信息"`
	Album       string   `json:"album,omitempty" jsonschema:"搜索结果中的专辑名"`
	AlbumArtist string   `json:"albumArtist,omitempty" jsonschema:"专辑艺术家"`
	Date        string   `json:"date,omitempty" jsonschema:"发行日期"`
	CoverURL    string   `json:"coverUrl,omitempty" jsonschema:"搜索结果中的封面地址"`
	DurationMS  int64    `json:"durationMs,omitempty" jsonschema:"搜索结果中的歌曲时长，单位毫秒"`
}
type downloadOutput struct {
	Result model.DownloadTaskResult `json:"result"`
}

type taskInput struct {
	DownloadID int64 `json:"downloadId" jsonschema:"下载历史中的任务 ID"`
}
type taskOutput struct {
	Result model.DownloadTaskResult `json:"result"`
}

type searchLibraryInput struct {
	Query string `json:"query" jsonschema:"歌曲名或歌手名，用于搜索本地曲库"`
	Limit int    `json:"limit,omitempty" jsonschema:"返回数量，默认 20，最大 100"`
}
type searchLibraryOutput struct {
	Songs []model.Song `json:"songs"`
	Count int          `json:"count"`
}

type listInput struct {
	Limit  int `json:"limit,omitempty" jsonschema:"返回数量，默认 50，最大 500"`
	Offset int `json:"offset,omitempty" jsonschema:"偏移量，默认 0"`
}
type downloadsOutput struct {
	Downloads []model.DownloadHistory `json:"downloads"`
}

// New 创建无状态的 Streamable HTTP MCP 服务。
func New(music *service.Music, logger *slog.Logger, token string) http.Handler {
	server := &Server{
		music: music, logger: logger,
		mcp: protocol.NewServer(&protocol.Implementation{Name: "tune-go", Version: "0.2.0"}, nil),
	}
	server.registerTools()
	handler := protocol.NewStreamableHTTPHandler(
		func(*http.Request) *protocol.Server { return server.mcp },
		&protocol.StreamableHTTPOptions{Stateless: true, JSONResponse: true, Logger: logger},
	)
	if token == "" {
		return handler
	}
	return mcpAuth(handler, token)
}

// mcpAuth 校验 Authorization: Bearer <token>，token 不匹配时返回 401。
func mcpAuth(next http.Handler, token string) http.Handler {
	expected := "Bearer " + token
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != expected {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusUnauthorized)
			w.Write([]byte(`{"error":"unauthorized","message":"invalid MCP token"}`))
			return
		}
		next.ServeHTTP(w, r)
	})
}

func (s *Server) registerTools() {
	// MCP 只负责参数和协议转换，具体业务统一交给 service.Music。
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "list_providers", Description: "列出 Tune Go 已注册的音乐源及其可用状态。"}, s.listProviders)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "search_music", Description: "从指定音乐源搜索歌曲；返回歌曲 ID 和可选音质。"}, s.searchMusic)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "search_library", Description: "查询本地曲库中已存在的歌曲；下载前应调用此工具检查，避免重复下载。"}, s.searchLibrary)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "resolve_music", Description: "解析临时资源信息，仅用于检查资源，不写入文件。"}, s.resolveMusic)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "download_music", Description: "创建异步下载任务，立即返回任务 ID。"}, s.downloadMusic)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "list_downloads", Description: "查询下载任务的状态、进度、字节数和重试次数。"}, s.listDownloads)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "pause_download", Description: "暂停下载任务并保留断点。"}, s.pauseDownload)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "resume_download", Description: "继续已暂停的下载任务。"}, s.resumeDownload)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "cancel_download", Description: "取消下载任务并删除断点。"}, s.cancelDownload)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "retry_download", Description: "重试失败或已取消的下载任务。"}, s.retryDownload)
	protocol.AddTool(s.mcp, &protocol.Tool{Name: "delete_download", Description: "删除下载任务记录，不删除已完成的音乐文件。"}, s.deleteDownload)
}

func (s *Server) listProviders(context.Context, *protocol.CallToolRequest, emptyInput) (*protocol.CallToolResult, providersOutput, error) {
	return nil, providersOutput{Providers: s.music.Providers()}, nil
}

func (s *Server) searchMusic(ctx context.Context, _ *protocol.CallToolRequest, input searchInput) (*protocol.CallToolResult, searchOutput, error) {
	if strings.TrimSpace(input.Query) == "" {
		return nil, searchOutput{}, errors.New("query is required")
	}
	if input.Page == 0 {
		input.Page = 1
	}
	if input.Limit == 0 {
		input.Limit = 20
	}
	if input.Page < 1 || input.Page > 10000 {
		return nil, searchOutput{}, errors.New("page must be between 1 and 10000")
	}
	if input.Limit < 1 || input.Limit > 100 {
		return nil, searchOutput{}, errors.New("limit must be between 1 and 100")
	}
	result, err := s.music.Search(ctx, model.SearchRequest{Provider: input.Provider, Query: input.Query, Page: input.Page, Limit: input.Limit, Lossless: input.Lossless})
	if err != nil {
		s.logger.Error("mcp search failed", "provider", input.Provider, "error", err)
		return nil, searchOutput{}, err
	}
	return nil, searchOutput{Result: result}, nil
}

func (s *Server) searchLibrary(ctx context.Context, _ *protocol.CallToolRequest, input searchLibraryInput) (*protocol.CallToolResult, searchLibraryOutput, error) {
	limit := int64(20)
	if input.Limit > 0 {
		if input.Limit > 100 {
			return nil, searchLibraryOutput{}, errors.New("limit must be between 1 and 100")
		}
		limit = int64(input.Limit)
	}
	songs, err := s.music.SearchSongs(ctx, strings.TrimSpace(input.Query), 0, limit)
	if err != nil {
		s.logger.Error("mcp library search failed", "query", input.Query, "error", err)
		return nil, searchLibraryOutput{}, err
	}
	return nil, searchLibraryOutput{Songs: songs, Count: len(songs)}, nil
}

func (s *Server) resolveMusic(ctx context.Context, _ *protocol.CallToolRequest, input resolveInput) (*protocol.CallToolResult, resolveOutput, error) {
	resource, err := s.music.Resolve(ctx, model.ResolveRequest{Provider: input.Provider, TrackID: input.TrackID, Quality: input.Quality})
	if err != nil {
		return nil, resolveOutput{}, err
	}
	return nil, resolveOutput{Resource: resource}, nil
}

func (s *Server) downloadMusic(ctx context.Context, _ *protocol.CallToolRequest, input downloadInput) (*protocol.CallToolResult, downloadOutput, error) {
	result, err := s.music.Download(ctx, model.DownloadRequest{
		ResolveRequest: model.ResolveRequest{
			Provider: input.Provider,
			TrackID:  input.TrackID,
			Quality:  input.Quality,
		},
		Metadata: model.MusicMetadata{
			Name:        input.Name,
			Artists:     input.Artists,
			Album:       input.Album,
			AlbumArtist: input.AlbumArtist,
			Date:        input.Date,
			CoverURL:    input.CoverURL,
		},
	})
	if err != nil {
		return nil, downloadOutput{}, err
	}
	return nil, downloadOutput{Result: result}, nil
}

func (s *Server) listDownloads(ctx context.Context, _ *protocol.CallToolRequest, input listInput) (*protocol.CallToolResult, downloadsOutput, error) {
	limit, err := normalizeListLimit(input.Limit)
	if err != nil {
		return nil, downloadsOutput{}, err
	}
	rows, err := s.music.ListDownloads(ctx, int64(limit), int64(input.Offset))
	if err != nil {
		return nil, downloadsOutput{}, err
	}
	return nil, downloadsOutput{Downloads: rows}, nil
}

func (s *Server) pauseDownload(ctx context.Context, _ *protocol.CallToolRequest, input taskInput) (*protocol.CallToolResult, taskOutput, error) {
	result, err := s.music.PauseDownload(ctx, input.DownloadID)
	return nil, taskOutput{Result: result}, err
}

func (s *Server) resumeDownload(ctx context.Context, _ *protocol.CallToolRequest, input taskInput) (*protocol.CallToolResult, taskOutput, error) {
	result, err := s.music.ResumeDownload(ctx, input.DownloadID)
	return nil, taskOutput{Result: result}, err
}

func (s *Server) cancelDownload(ctx context.Context, _ *protocol.CallToolRequest, input taskInput) (*protocol.CallToolResult, taskOutput, error) {
	result, err := s.music.CancelDownload(ctx, input.DownloadID)
	return nil, taskOutput{Result: result}, err
}

func (s *Server) retryDownload(ctx context.Context, _ *protocol.CallToolRequest, input taskInput) (*protocol.CallToolResult, taskOutput, error) {
	result, err := s.music.RetryDownload(ctx, input.DownloadID)
	return nil, taskOutput{Result: result}, err
}

func (s *Server) deleteDownload(ctx context.Context, _ *protocol.CallToolRequest, input taskInput) (*protocol.CallToolResult, taskOutput, error) {
	err := s.music.DeleteDownload(ctx, input.DownloadID)
	return nil, taskOutput{Result: model.DownloadTaskResult{DownloadID: input.DownloadID, Status: "deleted"}}, err
}

func normalizeListLimit(limit int) (int, error) {
	if limit == 0 {
		return 50, nil
	}
	if limit < 1 || limit > 500 {
		return 0, errors.New("limit must be between 1 and 500")
	}
	return limit, nil
}
