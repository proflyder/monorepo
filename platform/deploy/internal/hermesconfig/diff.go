package hermesconfig

import (
	"fmt"
	"strings"
)

// unifiedDiff is a small difflib-compatible unified diff. It only has to make a
// dry-run reviewable; the Python renderer's exact hunk boundaries are not part
// of the contract.
func unifiedDiff(path, oldText, newText string) string {
	oldLines := splitKeepEnds(oldText)
	newLines := splitKeepEnds(newText)
	operations := diffOperations(oldLines, newLines)
	if len(operations) == 0 {
		return ""
	}

	const context = 3
	var output strings.Builder
	fmt.Fprintf(&output, "--- %s\n+++ %s (rendered)\n", path, path)
	for _, hunk := range hunks(operations, context) {
		oldStart, oldCount := hunkRange(operations, hunk.start, hunk.end, "old")
		newStart, newCount := hunkRange(operations, hunk.start, hunk.end, "new")
		fmt.Fprintf(&output, "@@ -%d,%d +%d,%d @@\n", oldStart, oldCount, newStart, newCount)
		for _, operation := range operations[hunk.start:hunk.end] {
			output.WriteString(operation.prefix)
			output.WriteString(operation.text)
			if !strings.HasSuffix(operation.text, "\n") {
				output.WriteString("\n\\ No newline at end of file\n")
			}
		}
	}
	return output.String()
}

type diffOperation struct {
	kind   byte // ' ', '-' or '+'
	prefix string
	text   string
}

func splitKeepEnds(text string) []string {
	if text == "" {
		return nil
	}
	lines := strings.SplitAfter(text, "\n")
	if lines[len(lines)-1] == "" {
		lines = lines[:len(lines)-1]
	}
	return lines
}

// diffOperations computes a longest-common-subsequence diff.
func diffOperations(oldLines, newLines []string) []diffOperation {
	rows, columns := len(oldLines), len(newLines)
	table := make([][]int, rows+1)
	for index := range table {
		table[index] = make([]int, columns+1)
	}
	for i := rows - 1; i >= 0; i-- {
		for j := columns - 1; j >= 0; j-- {
			if oldLines[i] == newLines[j] {
				table[i][j] = table[i+1][j+1] + 1
			} else if table[i+1][j] >= table[i][j+1] {
				table[i][j] = table[i+1][j]
			} else {
				table[i][j] = table[i][j+1]
			}
		}
	}
	var operations []diffOperation
	i, j := 0, 0
	for i < rows && j < columns {
		switch {
		case oldLines[i] == newLines[j]:
			operations = append(operations, diffOperation{kind: ' ', prefix: " ", text: oldLines[i]})
			i++
			j++
		case table[i+1][j] >= table[i][j+1]:
			operations = append(operations, diffOperation{kind: '-', prefix: "-", text: oldLines[i]})
			i++
		default:
			operations = append(operations, diffOperation{kind: '+', prefix: "+", text: newLines[j]})
			j++
		}
	}
	for ; i < rows; i++ {
		operations = append(operations, diffOperation{kind: '-', prefix: "-", text: oldLines[i]})
	}
	for ; j < columns; j++ {
		operations = append(operations, diffOperation{kind: '+', prefix: "+", text: newLines[j]})
	}
	return operations
}

type hunkSpan struct {
	start int
	end   int
}

// hunks groups operations, keeping `context` unchanged lines around a change.
func hunks(operations []diffOperation, context int) []hunkSpan {
	var result []hunkSpan
	index := 0
	for index < len(operations) {
		if operations[index].kind == ' ' {
			index++
			continue
		}
		start := index - context
		if start < 0 {
			start = 0
		}
		end := index
		for end < len(operations) {
			if operations[end].kind != ' ' {
				end++
				continue
			}
			next := end
			for next < len(operations) && operations[next].kind == ' ' {
				next++
			}
			if next < len(operations) && next-end <= 2*context {
				end = next
				continue
			}
			end += context
			if end > len(operations) {
				end = len(operations)
			}
			break
		}
		result = append(result, hunkSpan{start: start, end: end})
		index = end
	}
	return result
}

// hunkRange returns the 1-based start line and line count for one side.
func hunkRange(operations []diffOperation, start, end int, side string) (int, int) {
	oldLine, newLine := 1, 1
	for _, operation := range operations[:start] {
		if operation.kind != '+' {
			oldLine++
		}
		if operation.kind != '-' {
			newLine++
		}
	}
	line := oldLine
	if side == "new" {
		line = newLine
	}
	count := 0
	for _, operation := range operations[start:end] {
		switch {
		case operation.kind == ' ':
			count++
		case side == "old" && operation.kind == '-':
			count++
		case side == "new" && operation.kind == '+':
			count++
		}
	}
	return line, count
}
