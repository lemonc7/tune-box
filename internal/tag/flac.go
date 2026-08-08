// Package tag 负责把歌曲信息直接写入音频文件，不生成额外的图片或歌词文件。
package tag

import (
	"bufio"
	"bytes"
	"context"
	"encoding/binary"
	"errors"
	"fmt"
	"image"
	_ "image/gif"
	_ "image/jpeg"
	_ "image/png"
	"io"
	"mime"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"

	"github.com/lemonc7/tune-go/internal/model"
)

const (
	maxCoverBytes = 5 << 20
	maxBlockBytes = 0xffffff
)

type flacBlock struct {
	typeID byte
	data   []byte
}

// Embedder 下载封面并以原子替换方式写入 FLAC 元数据。
type Embedder struct{ client *http.Client }

func New(client *http.Client) *Embedder { return &Embedder{client: client} }

// EmbedFLAC 写入 TITLE、ARTIST、ALBUM、LYRICS 和封面 Picture Block。
func (e *Embedder) EmbedFLAC(
	ctx context.Context,
	path string,
	metadata model.MusicMetadata,
	lyrics string,
) error {
	var cover []byte
	var coverType string
	if strings.TrimSpace(metadata.CoverURL) != "" {
		u, err := url.Parse(metadata.CoverURL)
		if err != nil || (u.Scheme != "http" && u.Scheme != "https") || u.Hostname() == "" {
			return fmt.Errorf("provider returned an invalid cover URL")
		}
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String(), nil)
		if err != nil {
			return err
		}

		resp, err := e.client.Do(req)
		if err != nil {
			return fmt.Errorf("download cover: %w", err)
		}
		defer resp.Body.Close()
		if resp.StatusCode < 200 || resp.StatusCode >= 300 {
			return fmt.Errorf("cover server returned HTTP %d", resp.StatusCode)
		}

		cover, err = io.ReadAll(io.LimitReader(resp.Body, maxCoverBytes+1))
		if err != nil {
			return fmt.Errorf("read cover: %w", err)
		}
		if len(cover) == 0 || len(cover) > maxCoverBytes {
			return fmt.Errorf("cover is empty or exceeds 5 MiB")
		}
		coverType, _, _ = mime.ParseMediaType(resp.Header.Get("Content-Type"))
		if coverType == "" || coverType == "application/octet-stream" {
			coverType = http.DetectContentType(cover)
		}
		if coverType != "image/jpeg" && coverType != "image/png" && coverType != "image/gif" {
			return fmt.Errorf("unsupported cover type %q", coverType)
		}
	}

	input, err := os.Open(path)
	if err != nil {
		return err
	}
	defer input.Close()
	reader := bufio.NewReader(input)
	magic := make([]byte, 4)
	if _, err := io.ReadFull(reader, magic); err != nil || string(magic) != "fLaC" {
		return fmt.Errorf("file is not a valid FLAC stream")
	}

	// 保存需要继续写回的metadata block
	blocks := make([]flacBlock, 0, 8)
	vendor := "TuneGo"
	// 原有文字标签
	comments := make([]string, 0, 16)
	for {
		header := make([]byte, 4)
		if _, err := io.ReadFull(reader, header); err != nil {
			return fmt.Errorf("read FLAC metadata header: %w", err)
		}
		// 第1字节：记录是否为最后一个block(最高位)和block类型(低7位)
		last := header[0]&0x80 != 0
		typeID := header[0] & 0x7f
		// 第2~4字节，记录长度
		length := int(header[1])<<16 | int(header[2])<<8 | int(header[3])
		// 读取block内容
		data := make([]byte, length)
		if _, err := io.ReadFull(reader, data); err != nil {
			return fmt.Errorf("read FLAC metadata block: %w", err)
		}
		switch typeID {
		// Vorbis Comment
		case 4:
			parsedVendor, parsedComments, err := parseVorbisComment(data)
			if err != nil {
				return err
			}
			vendor, comments = parsedVendor, parsedComments
		// Picture
		case 6:
			if len(cover) == 0 {
				blocks = append(blocks, flacBlock{typeID: typeID, data: data})
			}
		default:
			blocks = append(blocks, flacBlock{typeID: typeID, data: data})
		}
		if last {
			break
		}
	}
	if len(blocks) == 0 || blocks[0].typeID != 0 || len(blocks[0].data) != 34 {
		return fmt.Errorf("FLAC STREAMINFO block is missing or invalid")
	}

	replacements := map[string]string{
		"TITLE":       strings.TrimSpace(metadata.Name),
		"ARTIST":      strings.Join(metadata.Artists, "; "),
		"ALBUM":       strings.TrimSpace(metadata.Album),
		"LYRICS":      strings.TrimSpace(lyrics),
		"ALBUMARTIST": strings.TrimSpace(metadata.AlbumArtist),
		"DATE":        strings.TrimSpace(metadata.Date),
	}
	kept := comments[:0]
	for _, comment := range comments {
		key, _, found := strings.Cut(comment, "=")
		replacement, replace := replacements[strings.ToUpper(key)]
		if !found || !replace || replacement == "" {
			kept = append(kept, comment)
		}
	}
	comments = kept
	for _, key := range []string{"TITLE", "ARTIST", "ALBUM", "LYRICS", "ALBUMARTIST", "DATE"} {
		if value := replacements[key]; value != "" {
			comments = append(comments, key+"="+value)
		}
	}
	commentBlock, err := buildVorbisComment(vendor, comments)
	if err != nil {
		return err
	}
	blocks = append(blocks, flacBlock{typeID: 4, data: commentBlock})
	if len(cover) > 0 {
		pictureBlock, err := buildPicture(cover, coverType)
		if err != nil {
			return err
		}
		blocks = append(blocks, flacBlock{typeID: 6, data: pictureBlock})
	}

	info, err := input.Stat()
	if err != nil {
		return err
	}
	temp, err := os.CreateTemp(filepath.Dir(path), ".tagging-*.part")
	if err != nil {
		return err
	}
	tempPath := temp.Name()
	defer os.Remove(tempPath)
	if err := temp.Chmod(info.Mode()); err != nil {
		temp.Close()
		return err
	}
	if _, err := temp.WriteString("fLaC"); err != nil {
		temp.Close()
		return err
	}
	for index, block := range blocks {
		if len(block.data) > maxBlockBytes {
			temp.Close()
			return fmt.Errorf("FLAC metadata block exceeds 24-bit size limit")
		}
		header := []byte{block.typeID, byte(len(block.data) >> 16), byte(len(block.data) >> 8), byte(len(block.data))}
		if index == len(blocks)-1 {
			header[0] |= 0x80
		}
		if _, err := temp.Write(header); err != nil {
			temp.Close()
			return err
		}
		if _, err := temp.Write(block.data); err != nil {
			temp.Close()
			return err
		}
	}
	if err := copyWithContext(ctx, temp, reader); err != nil {
		temp.Close()
		return err
	}
	if err := temp.Sync(); err != nil {
		temp.Close()
		return err
	}
	if err := temp.Close(); err != nil {
		return err
	}
	if err := input.Close(); err != nil {
		return err
	}
	if err := ctx.Err(); err != nil {
		return err
	}
	if err := os.Rename(tempPath, path); err != nil {
		return err
	}
	return nil
}

func parseVorbisComment(data []byte) (string, []string, error) {
	readString := func(buffer *bytes.Reader) (string, error) {
		var length uint32
		if err := binary.Read(buffer, binary.LittleEndian, &length); err != nil {
			return "", err
		}
		if uint64(length) > uint64(buffer.Len()) {
			return "", fmt.Errorf("invalid FLAC Vorbis comment length")
		}
		value := make([]byte, length)
		_, err := io.ReadFull(buffer, value)
		return string(value), err
	}
	buffer := bytes.NewReader(data)
	vendor, err := readString(buffer)
	if err != nil {
		return "", nil, fmt.Errorf("read FLAC Vorbis vendor: %w", err)
	}
	var count uint32
	if err := binary.Read(buffer, binary.LittleEndian, &count); err != nil {
		return "", nil, fmt.Errorf("read FLAC Vorbis comment count: %w", err)
	}
	if uint64(count) > uint64(buffer.Len()/4) {
		return "", nil, fmt.Errorf("invalid FLAC Vorbis comment count")
	}
	comments := make([]string, 0, count)
	for i := uint32(0); i < count; i++ {
		comment, err := readString(buffer)
		if err != nil {
			return "", nil, fmt.Errorf("read FLAC Vorbis comment: %w", err)
		}
		comments = append(comments, comment)
	}
	return vendor, comments, nil
}

func buildVorbisComment(vendor string, comments []string) ([]byte, error) {
	var buffer bytes.Buffer
	writeString := func(value string) error {
		if len(value) > maxBlockBytes {
			return fmt.Errorf("FLAC Vorbis comment is too large")
		}
		if err := binary.Write(&buffer, binary.LittleEndian, uint32(len(value))); err != nil {
			return err
		}
		_, err := buffer.WriteString(value)
		return err
	}
	if err := writeString(vendor); err != nil {
		return nil, err
	}
	if err := binary.Write(&buffer, binary.LittleEndian, uint32(len(comments))); err != nil {
		return nil, err
	}
	for _, comment := range comments {
		if err := writeString(comment); err != nil {
			return nil, err
		}
	}
	return buffer.Bytes(), nil
}

func buildPicture(data []byte, mediaType string) ([]byte, error) {
	config, _, err := image.DecodeConfig(bytes.NewReader(data))
	if err != nil {
		return nil, fmt.Errorf("decode cover: %w", err)
	}
	var buffer bytes.Buffer
	values := []uint32{3, uint32(len(mediaType))}
	for _, value := range values {
		binary.Write(&buffer, binary.BigEndian, value)
	}
	buffer.WriteString(mediaType)
	for _, value := range []uint32{0, uint32(config.Width), uint32(config.Height), 24, 0, uint32(len(data))} {
		binary.Write(&buffer, binary.BigEndian, value)
	}
	buffer.Write(data)
	return buffer.Bytes(), nil
}

func copyWithContext(ctx context.Context, destination io.Writer, source io.Reader) error {
	buffer := make([]byte, 256*1024)
	for {
		if err := ctx.Err(); err != nil {
			return err
		}
		n, readErr := source.Read(buffer)
		if n > 0 {
			if _, err := destination.Write(buffer[:n]); err != nil {
				return err
			}
		}
		if readErr != nil {
			if errors.Is(readErr, io.EOF) {
				return nil
			}
			return readErr
		}
	}
}

type FLACInfo struct {
	Title   string
	Artists []string
}

func ReadFLACMetadata(path string) (FLACInfo, error) {
	input, err := os.Open(path)
	if err != nil {
		return FLACInfo{}, err
	}
	defer input.Close()
	reader := bufio.NewReader(input)
	magic := make([]byte, 4)
	if _, err := io.ReadFull(reader, magic); err != nil || string(magic) != "fLaC" {
		return FLACInfo{}, fmt.Errorf("not a valid FLAC file")
	}
	for {
		header := make([]byte, 4)
		if _, err := io.ReadFull(reader, header); err != nil {
			return FLACInfo{}, fmt.Errorf("read FLAC header: %w", err)
		}
		last := header[0]&0x80 != 0
		typeID := header[0] & 0x7f
		length := int(header[1])<<16 | int(header[2])<<8 | int(header[3])
		data := make([]byte, length)
		if _, err := io.ReadFull(reader, data); err != nil {
			return FLACInfo{}, fmt.Errorf("read FLAC block: %w", err)
		}
		if typeID == 4 {
			_, comments, err := parseVorbisComment(data)
			if err != nil {
				return FLACInfo{}, err
			}
			info := FLACInfo{}
			for _, c := range comments {
				key, value, found := strings.Cut(c, "=")
				if !found {
					continue
				}
				switch strings.ToUpper(key) {
				case "TITLE":
					info.Title = strings.TrimSpace(value)
				case "ARTIST":
					info.Artists = splitArtists(value)
				}
			}
			return info, nil
		}
		if last {
			break
		}
	}
	return FLACInfo{}, nil
}

func splitArtists(value string) []string {
	parts := strings.Split(value, ";")
	result := make([]string, 0, len(parts))
	for _, p := range parts {
		if p = strings.TrimSpace(p); p != "" {
			result = append(result, p)
		}
	}
	return result
}
