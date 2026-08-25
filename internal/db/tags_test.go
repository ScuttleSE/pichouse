package db

import (
	"testing"
	"time"

	"git.hemmalab.se/scuttle/pichouse/internal/model"
)

func newTestPhoto(t *testing.T, l *Library, dir string) int64 {
	t.Helper()
	fid, err := l.UpsertFolder(model.Folder{Path: dir + "/f", Name: "f", MTime: time.Now(), Year: 2024})
	if err != nil {
		t.Fatal(err)
	}
	pid, err := l.UpsertPhoto(model.Photo{FolderID: fid, Path: dir + "/f/a.jpg", Filename: "a.jpg", ModTime: time.Now()})
	if err != nil {
		t.Fatal(err)
	}
	return pid
}

func TestTagsAddSearch(t *testing.T) {
	dir := t.TempDir()
	l, err := OpenLibraryAt(dir + "/library.db")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	pid := newTestPhoto(t, l, dir)

	if err := l.AddPhotoTags(pid, []string{"Beach", "sunset", "dog"}, model.TagSourceAI); err != nil {
		t.Fatal(err)
	}
	if err := l.AddPhotoTags(pid, []string{"vacation"}, model.TagSourceUser); err != nil {
		t.Fatal(err)
	}
	tags, err := l.PhotoTags(pid)
	if err != nil {
		t.Fatal(err)
	}
	if len(tags) != 4 {
		t.Fatalf("want 4 tags got %d", len(tags))
	}

	ids, _ := l.SearchPhotoIDsByTag("dog")
	if !ids[pid] {
		t.Fatal("search dog missed")
	}
	ids, _ = l.SearchPhotoIDsByTag("sun") // prefix
	if !ids[pid] {
		t.Fatal("prefix search sun missed")
	}
}

func TestTagsRenameMergeDelete(t *testing.T) {
	dir := t.TempDir()
	l, err := OpenLibraryAt(dir + "/library.db")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	pid := newTestPhoto(t, l, dir)
	if err := l.AddPhotoTags(pid, []string{"beach", "dog"}, model.TagSourceAI); err != nil {
		t.Fatal(err)
	}

	if err := l.RenameTag("dog", "cat"); err != nil {
		t.Fatal(err)
	}
	if ids, _ := l.SearchPhotoIDsByTag("cat"); !ids[pid] {
		t.Fatal("rename: cat not found")
	}
	if ids, _ := l.SearchPhotoIDsByTag("dog"); ids[pid] {
		t.Fatal("rename: old dog still found")
	}

	if err := l.MergeTags("cat", "beach"); err != nil {
		t.Fatal(err)
	}
	if tags, _ := l.PhotoTags(pid); len(tags) != 1 {
		t.Fatalf("merge: want 1 tag got %d", len(tags))
	}

	if err := l.RemovePhotoTag(pid, "beach"); err != nil {
		t.Fatal(err)
	}
	if ids, _ := l.SearchPhotoIDsByTag("beach"); ids[pid] {
		t.Fatal("remove: beach still found")
	}
}

func TestAIStatusAndNeeding(t *testing.T) {
	dir := t.TempDir()
	l, err := OpenLibraryAt(dir + "/library.db")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	pid := newTestPhoto(t, l, dir)

	ids, _ := l.PhotosNeedingTags(0, false)
	if len(ids) != 1 {
		t.Fatalf("want 1 needing got %d", len(ids))
	}
	if err := l.SetAIStatus(pid, model.AIDone); err != nil {
		t.Fatal(err)
	}
	ids, _ = l.PhotosNeedingTags(0, false)
	if len(ids) != 0 {
		t.Fatalf("want 0 needing after done got %d", len(ids))
	}
}
