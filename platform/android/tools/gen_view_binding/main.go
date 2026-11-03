// Command gen_view_binding generates Kotlin ViewBinding classes from Android
// layout XML files.
//
// rules_android has no ViewBinding support and the androidx.viewbinding
// artifact is not part of the pinned Maven closure, so Bazel modules use this
// generator (through `view_binding = True` in `metro_android_library`). The
// output mirrors the AGP shape: `<LayoutName>Binding` in `<package>.databinding`
// with typed `val`s per `android:id`, plus `inflate(...)` and `bind(...)`.
// Bindings implement `dev.proflyder.home.core.ui.ViewBinding` instead of
// `androidx.viewbinding.ViewBinding`.
//
// Usage: gen_view_binding --package PKG --output FILE LAYOUT.xml...
package main

import (
	"encoding/xml"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"unicode"
	"unicode/utf8"
)

const androidNS = "http://schemas.android.com/apk/res/android"

var viewPackage = map[string]bool{
	"View":        true,
	"ViewStub":    true,
	"SurfaceView": true,
	"TextureView": true,
	"ViewGroup":   true,
}

var webkit = map[string]bool{"WebView": true}

// camel mirrors the Python splitting rule: [_\W]+ separates words, where the
// class is "underscore or not a word character".
func camel(name string, upperFirst bool) string {
	parts := splitWords(name)
	var b strings.Builder
	for _, part := range parts {
		r, size := utf8.DecodeRuneInString(part)
		b.WriteString(strings.ToUpper(string(r)))
		b.WriteString(part[size:])
	}
	out := b.String()
	if upperFirst {
		return out
	}
	r, size := utf8.DecodeRuneInString(out)
	return strings.ToLower(string(r)) + out[size:]
}

func splitWords(name string) []string {
	var parts []string
	var current strings.Builder
	flush := func() {
		if current.Len() > 0 {
			parts = append(parts, current.String())
			current.Reset()
		}
	}
	for _, r := range name {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			current.WriteRune(r)
		} else {
			flush()
		}
	}
	flush()
	return parts
}

func qualified(tag string) string {
	if strings.Contains(tag, ".") {
		return tag
	}
	if viewPackage[tag] {
		return "android.view." + tag
	}
	if webkit[tag] {
		return "android.webkit." + tag
	}
	return "android.widget." + tag
}

func tagName(name xml.Name) string {
	if name.Space == "" {
		return name.Local
	}
	return "{" + name.Space + "}" + name.Local
}

type view struct {
	name string
	tag  string
}

// collect returns (rootTag, views) in document order, deduplicated by id name.
func collect(r io.Reader) (string, []view, error) {
	decoder := xml.NewDecoder(r)
	rootTag := ""
	haveRoot := false
	seen := map[string]bool{}
	var views []view
	for {
		token, err := decoder.Token()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return "", nil, err
		}
		start, ok := token.(xml.StartElement)
		if !ok {
			continue
		}
		tag := tagName(start.Name)
		if !haveRoot {
			rootTag = tag
			haveRoot = true
		}
		raw := ""
		for _, attr := range start.Attr {
			if attr.Name.Space == androidNS && attr.Name.Local == "id" {
				raw = attr.Value
				break
			}
		}
		if !strings.HasPrefix(raw, "@+id/") && !strings.HasPrefix(raw, "@id/") {
			continue
		}
		parts := strings.SplitN(raw, "/", 2)
		if len(parts) != 2 {
			continue
		}
		name := parts[1]
		if seen[name] {
			continue
		}
		seen[name] = true
		elementTag := tag
		if elementTag == "include" {
			elementTag = "View"
		}
		views = append(views, view{name: name, tag: elementTag})
	}
	if !haveRoot {
		return "", nil, errors.New("no root element")
	}
	return rootTag, views, nil
}

type prop struct {
	prop string
	name string
	typ  string
}

func render(layoutPath string) (string, error) {
	layout := strings.TrimSuffix(filepath.Base(layoutPath), filepath.Ext(layoutPath))
	cls := camel(layout, true) + "Binding"

	file, err := os.Open(layoutPath)
	if err != nil {
		return "", err
	}
	defer file.Close()
	rootTag, views, err := collect(file)
	if err != nil {
		return "", fmt.Errorf("%s: %w", layoutPath, err)
	}
	if rootTag == "merge" {
		return "", fmt.Errorf("%s: <merge> roots are not supported", layoutPath)
	}
	rootType := qualified(rootTag)

	taken := map[string]bool{"root": true}
	var props []prop
	for _, item := range views {
		name := camel(item.name, false)
		for taken[name] {
			name += "View"
		}
		taken[name] = true
		props = append(props, prop{prop: name, name: item.name, typ: qualified(item.tag)})
	}

	var lines []string
	lines = append(lines, fmt.Sprintf("class %s private constructor(", cls))
	lines = append(lines, fmt.Sprintf("    override val root: %s,", rootType))
	for _, item := range props {
		lines = append(lines, fmt.Sprintf("    val %s: %s,", item.prop, item.typ))
	}
	lines = append(lines, ") : ViewBinding {")
	lines = append(lines, "    companion object {")
	lines = append(lines, "        fun inflate(")
	lines = append(lines, "            inflater: LayoutInflater,")
	lines = append(lines, "            parent: ViewGroup? = null,")
	lines = append(lines, "            attachToParent: Boolean = false,")
	lines = append(lines, fmt.Sprintf("        ): %s {", cls))
	lines = append(lines, fmt.Sprintf("            val root = inflater.inflate(R.layout.%s, parent, false)", layout))
	lines = append(lines, "            if (attachToParent && parent != null) parent.addView(root)")
	lines = append(lines, "            return bind(root)")
	lines = append(lines, "        }")
	lines = append(lines, "")
	lines = append(lines, fmt.Sprintf("        fun bind(view: View): %s = %s(", cls, cls))
	lines = append(lines, fmt.Sprintf("            view as %s,", rootType))
	for _, item := range props {
		lines = append(lines, fmt.Sprintf(
			"            view.findViewById<%s>(R.id.%s) ?: error(\"Missing view @id/%s in %s\"),",
			item.typ, item.name, item.name, layout,
		))
	}
	lines = append(lines, "        )")
	lines = append(lines, "    }")
	lines = append(lines, "}")
	return strings.Join(lines, "\n"), nil
}

func run(args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("gen_view_binding", flag.ContinueOnError)
	flags.SetOutput(stderr)
	pkg := flags.String("package", "", "generated Kotlin package")
	output := flags.String("output", "", "output file")
	if err := flags.Parse(args); err != nil {
		return 2
	}
	if *pkg == "" || *output == "" {
		fmt.Fprintln(stderr, "gen_view_binding: --package and --output are required")
		return 2
	}
	layouts := flags.Args()
	sort.Strings(layouts)

	var body []string
	for _, layout := range layouts {
		rendered, err := render(layout)
		if err != nil {
			fmt.Fprintln(stderr, err)
			return 1
		}
		body = append(body, rendered)
	}

	header := []string{
		"// Generated by //platform/android:gen_view_binding. Do not edit.",
		`@file:Suppress("unused")`,
		"",
		fmt.Sprintf("package %s.databinding", *pkg),
		"",
		"import android.view.LayoutInflater",
		"import android.view.View",
		"import android.view.ViewGroup",
		"import dev.proflyder.home.core.ui.ViewBinding",
		fmt.Sprintf("import %s.R", *pkg),
		"",
	}
	content := strings.Join(header, "\n") + "\n" + strings.Join(body, "\n\n") + "\n"
	if err := os.WriteFile(*output, []byte(content), 0o644); err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}
	return 0
}

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}
