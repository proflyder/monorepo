# OCI parity fixture

`layout/` is a minimal single-layer OCI image layout (`index.json` +
`oci-layout` + three `blobs/sha256/*` files) with a gzipped layer that contains
one `hello.txt`.

`pack.tar` and `docker.tar` are the archives the **Python** helper
`platform/ci/oci_image.py` produced from this layout with the reference
`ghcr.io/proflyder/golden:test`. The Go `oci` package must reproduce both
**byte for byte** and print the same manifest digest:

```
sha256:8883b693cf69f68bc4762c2764d610df8e6dd8388f6b6e536c0d90a50bf82880
```

`go test ./platform/deploy/internal/oci` asserts the bytes, the entry order,
mode 0644, mtime 0, the USTAR/PAX header shape and the record padding to 10240
bytes that `tarfile` applies.

The fixtures were captured before `oci_image.py` was deleted. To regenerate
after an intentional layout change, check the old helper out of the commit that
precedes the cutover (`git show 89c42de:platform/ci/oci_image.py`), generate the
archives, and update the digest constant in `internal/oci/oci_test.go`.
