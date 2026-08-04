package download

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"mime"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"strings"

	"github.com/lemonc7/tune-go/model"
)

var invalidFileChars = regexp.MustCompile(`[\\/:*?"<>|\x00-\x1f]`)

// Downloader 负责将 Provider 给出的临时资源安全地保存到本地。
type Downloader struct {
	client   *http.Client
	dir      string
	maxBytes int64
}

// New 创建下载器；maxBytes 是单个文件允许写入的最大字节数。
func New(client *http.Client, dir string, maxBytes int64) *Downloader {
	return &Downloader{client: client, dir: dir, maxBytes: maxBytes}
}

// Download 下载并校验音频。任何步骤失败都会删除临时文件，
// 只有完整校验通过后才会生成最终文件。
func (d *Downloader) Download(ctx context.Context, resource model.Resource, requestedName string) (model.DownloadResult, error) {
	// 下载地址只能来自 Provider，并且必须是合法的 HTTP(S) 地址。
	resourceURL, err := url.Parse(resource.URL)
	if err != nil || (resourceURL.Scheme != "http" && resourceURL.Scheme != "https") || resourceURL.Hostname() == "" {
		return model.DownloadResult{}, errors.New("provider returned an invalid resource URL")
	}
	if err := os.MkdirAll(d.dir, 0o755); err != nil {
		return model.DownloadResult{}, err
	}

	name := safeName(requestedName)
	if name == "" {
		name = resource.Provider + "-" + resource.TrackID
	}
	extension := "." + resource.Quality.Format
	if !strings.EqualFold(filepath.Ext(name), extension) {
		name += extension
	}
	target := filepath.Join(d.dir, name)
	// 先写入同目录临时文件，避免中断时留下看似完整的音频文件。
	temp, err := os.CreateTemp(d.dir, ".download-*.part")
	if err != nil {
		return model.DownloadResult{}, err
	}
	tempName := temp.Name()
	defer func() { temp.Close(); os.Remove(tempName) }()

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, resource.URL, nil)
	if err != nil {
		return model.DownloadResult{}, err
	}
	for key, value := range resource.Headers {
		req.Header.Set(key, value)
	}
	resp, err := d.client.Do(req)
	if err != nil {
		return model.DownloadResult{}, fmt.Errorf("download resource: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusPartialContent {
		return model.DownloadResult{}, fmt.Errorf("resource server returned HTTP %d", resp.StatusCode)
	}
	contentType := resp.Header.Get("Content-Type")
	mediaType, _, _ := mime.ParseMediaType(contentType)
	// 常见的接口错误页是 HTML 或 JSON，不能当作音频保存。
	if mediaType == "text/html" || mediaType == "application/json" || strings.HasPrefix(mediaType, "text/") {
		return model.DownloadResult{}, fmt.Errorf("resource server returned %s instead of audio", contentType)
	}
	if resp.ContentLength > d.maxBytes {
		return model.DownloadResult{}, fmt.Errorf("resource exceeds MAX_DOWNLOAD_BYTES")
	}

	// 即使服务器没有提供 Content-Length，也最多只读取上限加一个字节。
	written, err := io.Copy(temp, io.LimitReader(resp.Body, d.maxBytes+1))
	if err != nil {
		return model.DownloadResult{}, err
	}
	if written == 0 {
		return model.DownloadResult{}, errors.New("downloaded resource is empty")
	}
	if written > d.maxBytes {
		return model.DownloadResult{}, fmt.Errorf("resource exceeds MAX_DOWNLOAD_BYTES")
	}
	if resp.ContentLength >= 0 && written != resp.ContentLength {
		return model.DownloadResult{}, fmt.Errorf("incomplete download: expected %d bytes, received %d", resp.ContentLength, written)
	}
	if err := temp.Sync(); err != nil {
		return model.DownloadResult{}, err
	}
	if _, err := temp.Seek(0, io.SeekStart); err != nil {
		return model.DownloadResult{}, err
	}
	if resource.Quality.Format == "flac" {
		// FLAC 文件应包含 fLaC 标识，借此拦截伪装成音频的错误响应。
		header := make([]byte, 64*1024)
		n, _ := temp.Read(header)
		if !bytes.Contains(header[:n], []byte("fLaC")) {
			return model.DownloadResult{}, errors.New("resource is not a valid FLAC stream")
		}
	}
	if err := temp.Close(); err != nil {
		return model.DownloadResult{}, err
	}
	// 硬链接操作具有排他性，不会覆盖用户目录中已经存在的同名文件。
	if err := os.Link(tempName, target); err != nil {
		if errors.Is(err, os.ErrExist) {
			return model.DownloadResult{}, fmt.Errorf("target file already exists: %s", name)
		}
		return model.DownloadResult{}, err
	}
	if err := os.Remove(tempName); err != nil {
		return model.DownloadResult{}, err
	}

	absolute, _ := filepath.Abs(target)
	return model.DownloadResult{Path: absolute, FileName: name, Size: written, ContentType: contentType}, nil
}

func safeName(value string) string {
	// 清理各操作系统不允许出现在文件名中的字符，并限制名称长度。
	value = invalidFileChars.ReplaceAllString(strings.TrimSpace(value), "_")
	value = strings.Trim(value, ". ")
	if len([]rune(value)) > 180 {
		value = string([]rune(value)[:180])
	}
	return value
}
