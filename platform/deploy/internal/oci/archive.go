package oci

import (
	"fmt"
	"os"
)

// recordSize is the tar blocking factor Python's tarfile uses: after the two
// terminating zero blocks it pads the archive to a multiple of 10240 bytes.
// The padding is part of the archive's identity for byte-parity checks and is
// harmless for `ctr images import` and `docker load`.
const recordSize = 10240

// blockSize is one tar header/data block.
const blockSize = 512

// archive writes Python-compatible USTAR archives.
//
// The standard library's archive/tar writes `0000000\0` into the zero
// devmajor/devminor fields, while Python's tarfile leaves them as NUL bytes.
// The deploy path asserts byte equality with the historical archives (the
// digest guarantees the blobs, the bytes guarantee the archive), so the header
// is emitted here field by field instead of delegating to the writer.
type archive struct {
	file   *os.File
	offset int64
}

func newArchive(path string) (*archive, error) {
	file, err := os.Create(path)
	if err != nil {
		return nil, err
	}
	return &archive{file: file}, nil
}

// AddFile writes one regular file entry: mode 0644, mtime 0, uid/gid 0 and no
// owner names, exactly like Python's `tarfile.TarInfo`+`_add_bytes`.
func (a *archive) AddFile(name string, data []byte) error {
	header, err := ustarHeader(name, int64(len(data)))
	if err != nil {
		return err
	}
	if _, err := a.file.Write(header); err != nil {
		return err
	}
	if _, err := a.file.Write(data); err != nil {
		return err
	}
	if padding := (blockSize - len(data)%blockSize) % blockSize; padding > 0 {
		if _, err := a.file.Write(make([]byte, padding)); err != nil {
			return err
		}
	}
	a.offset += int64(len(header) + len(data) + (blockSize-len(data)%blockSize)%blockSize)
	return nil
}

// CloseWith finalizes the archive: two zero blocks and padding to the next
// 10240-byte record, mirroring `tarfile.close`.
func (a *archive) CloseWith(err error) error {
	if err == nil {
		terminator := make([]byte, 2*blockSize)
		if _, writeErr := a.file.Write(terminator); writeErr != nil {
			err = writeErr
		} else {
			a.offset += int64(len(terminator))
		}
	}
	if err == nil {
		if padding := (recordSize - a.offset%recordSize) % recordSize; padding > 0 {
			if _, writeErr := a.file.Write(make([]byte, padding)); writeErr != nil {
				err = writeErr
			}
		}
	}
	if closeErr := a.file.Close(); err == nil {
		err = closeErr
	}
	return err
}

// ustarHeader builds a POSIX ustar header for a regular file.
func ustarHeader(name string, size int64) ([]byte, error) {
	if len(name) > 100 {
		return nil, fmt.Errorf("tar entry name %q is longer than 100 bytes", name)
	}
	header := make([]byte, blockSize)
	copy(header[0:100], name)
	putOctal(header[100:108], 0o644, 7)
	putOctal(header[108:116], 0, 7)
	putOctal(header[116:124], 0, 7)
	putOctal(header[124:136], size, 11)
	putOctal(header[136:148], 0, 11)
	// The checksum field is spaces while the checksum is computed.
	for index := 148; index < 156; index++ {
		header[index] = ' '
	}
	header[156] = '0' // regular file
	copy(header[257:263], "ustar\x00")
	copy(header[263:265], "00")
	// uname, gname, devmajor, devminor and prefix stay NUL: Python writes an
	// empty field for a zero device number.
	checksum := 0
	for _, value := range header {
		checksum += int(value)
	}
	copy(header[148:154], fmt.Sprintf("%06o", checksum))
	header[154] = 0
	header[155] = ' '
	return header, nil
}

// putOctal writes a zero-padded octal number with a trailing NUL, the format
// Python's `itn` uses for fixed-width numeric fields.
func putOctal(field []byte, value int64, digits int) {
	text := fmt.Sprintf("%0*o", digits, value)
	copy(field, text)
	field[digits] = 0
}
