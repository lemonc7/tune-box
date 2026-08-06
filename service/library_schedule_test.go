package service

import (
	"testing"
	"time"
)

func TestNextDailyRun(t *testing.T) {
	location := time.FixedZone("test", 8*60*60)
	scanAt := 3*time.Hour + 15*time.Minute

	tests := []struct {
		name string
		now  time.Time
		want time.Time
	}{
		{
			name: "later today",
			now:  time.Date(2026, 8, 6, 2, 0, 0, 0, location),
			want: time.Date(2026, 8, 6, 3, 15, 0, 0, location),
		},
		{
			name: "tomorrow after scheduled time",
			now:  time.Date(2026, 8, 6, 4, 0, 0, 0, location),
			want: time.Date(2026, 8, 7, 3, 15, 0, 0, location),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := nextDailyRun(tt.now, scanAt, location); !got.Equal(tt.want) {
				t.Fatalf("nextDailyRun() = %s, want %s", got, tt.want)
			}
		})
	}
}
