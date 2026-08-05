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
	"strconv"
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

// Options 描述任务断点、进度回调以及最终落盘前的音频处理步骤。
type Options struct {
	TaskID         int64
	FileName       string
	ExpectedBytes  int64
	Progress       func(downloaded, total int64)
	BeforeFinalize func(ctx context.Context, path string) error
}

// New 创建下载器；maxBytes 是单个文件允许写入的最大字节数。
func New(client *http.Client, dir string, maxBytes int64) *Downloader {
	return &Downloader{client: client, dir: dir, maxBytes: maxBytes}
}

// Download 下载并校验音频。临时文件使用任务 ID 命名，中断后可以继续下载。
func (d *Downloader) Download(ctx context.Context, resource model.Resource, options Options) (model.DownloadResult, error) {
	// 下载地址只能来自 Provider，并且必须是合法的 HTTP(S) 地址。
	resourceURL, err := url.Parse(resource.URL)
	if err != nil || (resourceURL.Scheme != "http" && resourceURL.Scheme != "https") || resourceURL.Hostname() == "" {
		return model.DownloadResult{}, errors.New("provider returned an invalid resource URL")
	}
	if err := os.MkdirAll(d.dir, 0o755); err != nil {
		return model.DownloadResult{}, err
	}

	name := safeName(options.FileName)
	if name == "" {
		name = resource.Provider + "-" + resource.TrackID
	}
	extension := "." + resource.Quality.Format
	if !strings.EqualFold(filepath.Ext(name), extension) {
		name += extension
	}
	target := filepath.Join(d.dir, name)
	// 固定的临时文件名既不会被曲库扫描，也能让暂停和异常任务断点续传。
	tempName := d.partialPath(options.TaskID)
	temp, err := os.OpenFile(tempName, os.O_CREATE|os.O_RDWR, 0o644)
	if err != nil {
		return model.DownloadResult{}, err
	}
	defer temp.Close()
	offset, err := temp.Seek(0, io.SeekEnd)
	if err != nil {
		return model.DownloadResult{}, err
	}
	// 下载已经完整但标签阶段被进程中断时，直接重新执行标签阶段，不再请求音频资源。
	if options.ExpectedBytes > 0 && offset >= options.ExpectedBytes {
		if err := temp.Close(); err != nil {
			return model.DownloadResult{}, err
		}
		return d.finalize(ctx, tempName, target, name, "audio/"+resource.Quality.Format, resource, options)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, resource.URL, nil)
	if err != nil {
		return model.DownloadResult{}, err
	}
	for key, value := range resource.Headers {
		req.Header.Set(key, value)
	}
	if offset > 0 {
		req.Header.Set("Range", fmt.Sprintf("bytes=%d-", offset))
	}
	resp, err := d.client.Do(req)
	if err != nil {
		return model.DownloadResult{}, fmt.Errorf("download resource: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusPartialContent {
		return model.DownloadResult{}, fmt.Errorf("resource server returned HTTP %d", resp.StatusCode)
	}
	// 服务端不支持 Range 时会返回 200，此时必须从头覆盖，不能把完整文件追加到断点之后。
	if offset > 0 && resp.StatusCode == http.StatusOK {
		if err := temp.Truncate(0); err != nil {
			return model.DownloadResult{}, err
		}
		if _, err := temp.Seek(0, io.SeekStart); err != nil {
			return model.DownloadResult{}, err
		}
		offset = 0
	}
	contentType := resp.Header.Get("Content-Type")
	mediaType, _, _ := mime.ParseMediaType(contentType)
	// 常见的接口错误页是 HTML 或 JSON，不能当作音频保存。
	if mediaType == "text/html" || mediaType == "application/json" || strings.HasPrefix(mediaType, "text/") {
		return model.DownloadResult{}, fmt.Errorf("resource server returned %s instead of audio", contentType)
	}
	total := int64(0)
	if resp.ContentLength >= 0 {
		total = offset + resp.ContentLength
	}
	if value := resp.Header.Get("Content-Range"); value != "" {
		if slash := strings.LastIndexByte(value, '/'); slash >= 0 {
			if parsed, parseErr := strconv.ParseInt(value[slash+1:], 10, 64); parseErr == nil {
				total = parsed
			}
		}
	}
	if total > d.maxBytes {
		return model.DownloadResult{}, fmt.Errorf("resource exceeds MAX_DOWNLOAD_BYTES")
	}

	if options.Progress != nil {
		options.Progress(offset, total)
	}
	// 分块复制才能持续上报进度；无 Content-Length 时百分比保持 0，但字节数仍会更新。
	buffer := make([]byte, 128*1024)
	downloaded := offset
	for {
		n, readErr := resp.Body.Read(buffer)
		if n > 0 {
			if downloaded+int64(n) > d.maxBytes {
				return model.DownloadResult{}, fmt.Errorf("resource exceeds MAX_DOWNLOAD_BYTES")
			}
			if _, err := temp.Write(buffer[:n]); err != nil {
				return model.DownloadResult{}, err
			}
			downloaded += int64(n)
			if options.Progress != nil {
				options.Progress(downloaded, total)
			}
		}
		if readErr != nil {
			if errors.Is(readErr, io.EOF) {
				break
			}
			return model.DownloadResult{}, readErr
		}
	}
	if downloaded == 0 {
		return model.DownloadResult{}, errors.New("downloaded resource is empty")
	}
	if total > 0 && downloaded != total {
		return model.DownloadResult{}, fmt.Errorf("incomplete download: expected %d bytes, received %d", total, downloaded)
	}
	if err := temp.Sync(); err != nil {
		return model.DownloadResult{}, err
	}
	if err := temp.Close(); err != nil {
		return model.DownloadResult{}, err
	}
	return d.finalize(ctx, tempName, target, name, contentType, resource, options)
}

func (d *Downloader) finalize(ctx context.Context, tempName, target, name, contentType string, resource model.Resource, options Options) (model.DownloadResult, error) {
	if resource.Quality.Format == "flac" {
		file, err := os.Open(tempName)
		if err != nil {
			return model.DownloadResult{}, err
		}
		header := make([]byte, 64*1024)
		n, _ := file.Read(header)
		file.Close()
		if !bytes.Contains(header[:n], []byte("fLaC")) {
			return model.DownloadResult{}, errors.New("resource is not a valid FLAC stream")
		}
	}
	if options.BeforeFinalize != nil {
		if err := options.BeforeFinalize(ctx, tempName); err != nil {
			return model.DownloadResult{}, err
		}
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
	info, err := os.Stat(target)
	if err != nil {
		return model.DownloadResult{}, err
	}
	return model.DownloadResult{Path: absolute, FileName: name, Size: info.Size(), ContentType: contentType}, nil
}

// RemovePartial 删除取消或删除任务遗留的断点文件。
func (d *Downloader) RemovePartial(taskID int64) error {
	err := os.Remove(d.partialPath(taskID))
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	return err
}

func (d *Downloader) partialPath(taskID int64) string {
	return filepath.Join(d.dir, fmt.Sprintf(".download-%d.part", taskID))
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
