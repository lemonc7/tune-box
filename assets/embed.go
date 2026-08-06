package assets

import "embed"

//go:embed app.css htmx.min.js favicon.ico
var Assets embed.FS
