package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

const layoutFixture = `<?xml version="1.0" encoding="utf-8"?>
<LinearLayout xmlns:android="http://schemas.android.com/apk/res/android"
    android:layout_width="match_parent" android:layout_height="match_parent">
    <TextView android:id="@+id/item_title" />
    <View android:id="@id/divider" />
    <com.google.android.material.button.MaterialButton android:id="@+id/item_save" />
    <TextView android:id="@+id/item_title" />
    <TextView android:id="@+id/root" />
    <include layout="@layout/other" android:id="@+id/included" />
    <TextView android:text="no id" />
</LinearLayout>
`

func generate(t *testing.T, layouts map[string]string) (int, string, string) {
	t.Helper()
	temp := t.TempDir()
	paths := make([]string, 0, len(layouts))
	for name, body := range layouts {
		path := filepath.Join(temp, name)
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
		paths = append(paths, path)
	}
	output := filepath.Join(temp, "out.kt")
	var stdout, stderr bytes.Buffer
	args := append([]string{"--package", "a.b", "--output", output}, paths...)
	code := run(args, &stdout, &stderr)
	text, err := os.ReadFile(output)
	if err != nil {
		return code, "", stderr.String()
	}
	return code, string(text), stderr.String()
}

func TestGeneratesTypedProperties(t *testing.T) {
	code, kt, stderr := generate(t, map[string]string{"fragment_sample.xml": layoutFixture})
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
	for _, want := range []string{
		"package a.b.databinding",
		"import a.b.R",
		"class FragmentSampleBinding private constructor(",
		"override val root: android.widget.LinearLayout,",
		"val itemTitle: android.widget.TextView,",
		"val divider: android.view.View,",
		"val itemSave: com.google.android.material.button.MaterialButton,",
		"val included: android.view.View,",
		"R.layout.fragment_sample",
	} {
		if !strings.Contains(kt, want) {
			t.Errorf("output does not contain %q:\n%s", want, kt)
		}
	}
}

func TestDuplicateIDsAndRootNameClash(t *testing.T) {
	_, kt, _ := generate(t, map[string]string{"fragment_sample.xml": layoutFixture})
	if got := strings.Count(kt, "val itemTitle:"); got != 1 {
		t.Errorf("itemTitle declared %d times, want 1", got)
	}
	if !strings.Contains(kt, "val rootView: android.widget.TextView,") {
		t.Errorf("root name clash not renamed:\n%s", kt)
	}
}

func TestOneClassPerLayoutSorted(t *testing.T) {
	_, kt, _ := generate(t, map[string]string{
		"b_two.xml": layoutFixture,
		"a_one.xml": layoutFixture,
	})
	a := strings.Index(kt, "class AOneBinding")
	b := strings.Index(kt, "class BTwoBinding")
	if a < 0 || b < 0 || a >= b {
		t.Errorf("layouts not sorted (a=%d b=%d):\n%s", a, b, kt)
	}
}

func TestMergeRootIsRejected(t *testing.T) {
	code, _, stderr := generate(t, map[string]string{
		"x.xml": `<merge xmlns:android="http://schemas.android.com/apk/res/android"/>`,
	})
	if code == 0 {
		t.Fatalf("merge root accepted")
	}
	if !strings.Contains(stderr, "<merge> roots are not supported") {
		t.Errorf("missing merge error: %s", stderr)
	}
}
