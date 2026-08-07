package web

import (
	"net/http"
	"strings"

	"github.com/lemonc7/tune-go/internal/model"
)

func bindDownloadForm(r *http.Request) (model.DownloadRequest, error) {
	if err := r.ParseForm(); err != nil {
		return model.DownloadRequest{}, err
	}

	return model.DownloadRequest{
		ResolveRequest: model.ResolveRequest{
			Provider: strings.TrimSpace(r.FormValue("provider")),
			TrackID:  strings.TrimSpace(r.FormValue("trackId")),
			Quality:  strings.TrimSpace(r.FormValue("quality")),
		},
		Metadata: model.MusicMetadata{
			Name:        strings.TrimSpace(r.FormValue("name")),
			Artists:     cleanStrings(r.Form["artists"]),
			Album:       strings.TrimSpace(r.FormValue("album")),
			AlbumArtist: strings.TrimSpace(r.FormValue("albumArtist")),
			Date:        strings.TrimSpace(r.FormValue("date")),
			CoverURL:    strings.TrimSpace(r.FormValue("coverUrl")),
		},
	}, nil
}

func cleanStrings(values []string) []string {
	result := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value != "" {
			result = append(result, value)
		}
	}

	return result
}

func defaultValue(value, fallback string) string {
	if value == "" {
		return fallback
	}
	return value
}
