package main

import (
	"os"
)

func openInput(path string) (*os.File, error) {
	return os.OpenFile(path, os.O_RDONLY, 0)
}
