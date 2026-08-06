package config

import (
	"testing"
	"time"
)

func TestLibraryScanSchedule(t *testing.T) {
	cfg := LibraryConfig{ScanAt: "03:15", Timezone: "Asia/Shanghai"}
	scanAt, location, enabled, err := cfg.ScanSchedule()
	if err != nil {
		t.Fatal(err)
	}
	if !enabled {
		t.Fatal("expected schedule to be enabled")
	}
	if scanAt != 3*time.Hour+15*time.Minute {
		t.Fatalf("unexpected scan time: %s", scanAt)
	}
	if location.String() != "Asia/Shanghai" {
		t.Fatalf("unexpected location: %s", location)
	}
}

func TestLibraryScanScheduleDisabled(t *testing.T) {
	_, _, enabled, err := (LibraryConfig{}).ScanSchedule()
	if err != nil {
		t.Fatal(err)
	}
	if enabled {
		t.Fatal("expected empty scan time to disable schedule")
	}
}

func TestLibraryScanScheduleRejectsInvalidTime(t *testing.T) {
	_, _, _, err := (LibraryConfig{ScanAt: "25:00"}).ScanSchedule()
	if err == nil {
		t.Fatal("expected invalid scan time error")
	}
}
