package assets

import "embed"

//go:embed htmx.min.js favicon.ico app.css
var Assets embed.FS
