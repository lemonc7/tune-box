// Package mcpserver 将 Tune Go 的音乐搜索和下载能力暴露为 MCP 工具。
package mcp

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"strings"

	"github.com/lemonc7/tune-go/download"
	"github.com/lemonc7/tune-go/model"
	"github.com/lemonc7/tune-go/provider"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

// Server 组合 MCP 协议服务及 Tune Go 已有的业务组件。
type Server struct {
	registry   *provider.Registry
	downloader *download.Downloader
	logger     *slog.Logger
	mcp        *mcp.Server
}

type emptyInput struct{}

type providersOutput struct {
	Providers []provider.Info `json:"providers"`
}

type searchInput struct {
	Provider string `json:"provider" jsonschema:"音乐源名称，目前支持 kuwo 和 netease"`
	Query    string `json:"query" jsonschema:"歌曲名、歌手名或专辑名"`
	Page     int    `json:"page,omitempty" jsonschema:"页码，默认为 1，最大为 10000"`
	Limit    int    `json:"limit,omitempty" jsonschema:"每页数量，默认为 20，最大为 100"`
	Lossless bool   `json:"lossless,omitempty" jsonschema:"是否只返回包含无损音质的歌曲"`
}

type searchOutput struct {
	Result model.SearchResult `json:"result"`
}

type resolveInput struct {
	Provider string `json:"provider" jsonschema:"音乐源名称，目前支持 kuwo 和 netease"`
	TrackID  string `json:"trackId" jsonschema:"搜索结果中的歌曲 ID"`
	Quality  string `json:"quality" jsonschema:"搜索结果中的音质 ID，例如 2000kflac 或 lossless"`
}

type resolveOutput struct {
	Resource model.Resource `json:"resource"`
}

type downloadInput struct {
	Provider string `json:"provider" jsonschema:"音乐源名称，目前支持 kuwo 和 netease"`
	TrackID  string `json:"trackId" jsonschema:"搜索结果中的歌曲 ID"`
	Quality  string `json:"quality" jsonschema:"搜索结果中的音质 ID，例如 2000kflac 或 lossless"`
	FileName string `json:"fileName,omitempty" jsonschema:"可选的保存文件名，不需要包含扩展名"`
}

type downloadOutput struct {
	Result model.DownloadResult `json:"result"`
}

// New 创建一个使用 Streamable HTTP 传输的无状态 MCP Handler。
func New(registry *provider.Registry, downloader *download.Downloader, logger *slog.Logger) http.Handler {
	server := &Server{
		registry:   registry,
		downloader: downloader,
		logger:     logger,
		mcp: mcp.NewServer(&mcp.Implementation{
			Name:    "tune-go",
			Version: "0.1.0",
		}, nil),
	}
	server.registerTools()

	return mcp.NewStreamableHTTPHandler(
		func(*http.Request) *mcp.Server { return server.mcp },
		&mcp.StreamableHTTPOptions{
			Stateless:    true,
			JSONResponse: true,
			Logger:       logger,
		},
	)
}

func (s *Server) registerTools() {
	mcp.AddTool(s.mcp, &mcp.Tool{
		Name:        "list_providers",
		Description: "列出 Tune Go 已注册的音乐源及其可用状态。",
	}, s.listProviders)

	mcp.AddTool(s.mcp, &mcp.Tool{
		Name:        "search_music",
		Description: "从指定音乐源搜索歌曲；返回歌曲 ID 和可选音质，下载前应先调用此工具。",
	}, s.searchMusic)

	mcp.AddTool(s.mcp, &mcp.Tool{
		Name:        "resolve_music",
		Description: "将歌曲 ID 和音质 ID 解析为临时资源信息，仅用于检查资源，不会写入文件。",
	}, s.resolveMusic)

	mcp.AddTool(s.mcp, &mcp.Tool{
		Name:        "download_music",
		Description: "解析歌曲资源并同步下载到 Tune Go 配置的下载目录；调用可能耗时较长，并会占用网络流量和磁盘空间。",
	}, s.downloadMusic)
}

func (s *Server) listProviders(context.Context, *mcp.CallToolRequest, emptyInput) (*mcp.CallToolResult, providersOutput, error) {
	return nil, providersOutput{Providers: s.registry.List()}, nil
}

func (s *Server) searchMusic(ctx context.Context, _ *mcp.CallToolRequest, input searchInput) (*mcp.CallToolResult, searchOutput, error) {
	input.Query = strings.TrimSpace(input.Query)
	if input.Query == "" {
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

	item, err := s.registry.Get(input.Provider)
	if err != nil {
		return nil, searchOutput{}, err
	}
	result, err := item.Search(ctx, input.Query, input.Page, input.Limit)
	if err != nil {
		s.logger.Error("mcp search failed", "provider", input.Provider, "error", err)
		return nil, searchOutput{}, err
	}
	if input.Lossless {
		result = provider.FilterLossless(result)
	}
	return nil, searchOutput{Result: result}, nil
}

func (s *Server) resolveMusic(ctx context.Context, _ *mcp.CallToolRequest, input resolveInput) (*mcp.CallToolResult, resolveOutput, error) {
	resource, err := s.resolve(ctx, input.Provider, input.TrackID, input.Quality)
	if err != nil {
		s.logger.Error("mcp resolve failed", "provider", input.Provider, "trackId", input.TrackID, "error", err)
		return nil, resolveOutput{}, err
	}
	return nil, resolveOutput{Resource: resource}, nil
}

func (s *Server) downloadMusic(ctx context.Context, _ *mcp.CallToolRequest, input downloadInput) (*mcp.CallToolResult, downloadOutput, error) {
	resource, err := s.resolve(ctx, input.Provider, input.TrackID, input.Quality)
	if err != nil {
		s.logger.Error("mcp resolve failed", "provider", input.Provider, "trackId", input.TrackID, "error", err)
		return nil, downloadOutput{}, err
	}
	result, err := s.downloader.Download(ctx, resource, input.FileName)
	if err != nil {
		s.logger.Error("mcp download failed", "provider", input.Provider, "trackId", input.TrackID, "error", err)
		return nil, downloadOutput{}, err
	}
	return nil, downloadOutput{Result: result}, nil
}

func (s *Server) resolve(ctx context.Context, providerName, trackID, quality string) (model.Resource, error) {
	if strings.TrimSpace(trackID) == "" {
		return model.Resource{}, errors.New("trackId is required")
	}
	if strings.TrimSpace(quality) == "" {
		return model.Resource{}, errors.New("quality is required")
	}
	item, err := s.registry.Get(providerName)
	if err != nil {
		return model.Resource{}, err
	}
	resource, err := item.Resolve(ctx, trackID, quality)
	if err != nil {
		return model.Resource{}, fmt.Errorf("resolve resource: %w", err)
	}
	return resource, nil
}
