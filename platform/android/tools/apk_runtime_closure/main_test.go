package main

import (
	"archive/zip"
	"bytes"
	"encoding/binary"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func putU4(data []byte, offset int, value uint32) {
	binary.LittleEndian.PutUint32(data[offset:offset+4], value)
}

// buildDex creates the smallest DEX that defines the given descriptors.
func buildDex(descriptors []string) []byte {
	count := len(descriptors)
	stringIDsOff := 0x70
	typeIDsOff := stringIDsOff + 4*count
	classDefsOff := typeIDsOff + 4*count
	dataOff := classDefsOff + 32*count

	data := make([]byte, dataOff)
	copy(data, []byte("dex\n035\x00"))
	putU4(data, 0x38, uint32(count))
	putU4(data, 0x3C, uint32(stringIDsOff))
	putU4(data, 0x40, uint32(count))
	putU4(data, 0x44, uint32(typeIDsOff))
	putU4(data, 0x60, uint32(count))
	putU4(data, 0x64, uint32(classDefsOff))

	for index, descriptor := range descriptors {
		putU4(data, stringIDsOff+4*index, uint32(len(data)))
		// UTF-16 length is skipped by the parser; ASCII descriptors fit in one
		// uleb128 byte.
		data = append(data, byte(len(descriptor)))
		data = append(data, []byte(descriptor)...)
		data = append(data, 0)
		putU4(data, typeIDsOff+4*index, uint32(index))
		putU4(data, classDefsOff+32*index, uint32(index))
	}
	return data
}

func writeZip(t *testing.T, path string, files map[string][]byte) {
	t.Helper()
	file, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	writer := zip.NewWriter(file)
	for name, payload := range files {
		entry, err := writer.Create(name)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := entry.Write(payload); err != nil {
			t.Fatal(err)
		}
	}
	if err := writer.Close(); err != nil {
		t.Fatal(err)
	}
}

// buildClass writes a class file with a constant pool that names the class and
// every reference. The parser only reads the constant pool.
func buildClass(name string, refs []string) []byte {
	var pool []byte
	count := 1
	add := func(text string) int {
		index := count
		count++
		pool = append(pool, 1)
		pool = binary.BigEndian.AppendUint16(pool, uint16(len(text)))
		pool = append(pool, []byte(text)...)
		return index
	}
	nameIndex := add(name)
	pool = append(pool, 7)
	pool = binary.BigEndian.AppendUint16(pool, uint16(nameIndex))
	count++
	for _, ref := range refs {
		refIndex := add(ref)
		pool = append(pool, 7)
		pool = binary.BigEndian.AppendUint16(pool, uint16(refIndex))
		count++
	}

	data := []byte{0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52}
	data = binary.BigEndian.AppendUint16(data, uint16(count))
	data = append(data, pool...)
	return data
}

func runClosure(t *testing.T, args ...string) (int, string, string) {
	t.Helper()
	var stdout, stderr bytes.Buffer
	code := run(args, &stdout, &stderr)
	return code, stdout.String(), stderr.String()
}

func TestMainSuccess(t *testing.T) {
	temp := t.TempDir()
	apk := filepath.Join(temp, "app.apk")
	jar := filepath.Join(temp, "app_deploy.jar")
	writeZip(t, apk, map[string][]byte{"classes.dex": buildDex(requiredDexClasses)})
	writeZip(t, jar, map[string][]byte{"com/example/Foo.class": buildClass("com/example/Foo", nil)})

	code, stdout, stderr := runClosure(t, apk, jar)
	if code != 0 {
		t.Fatalf("exit %d: %s%s", code, stdout, stderr)
	}
	if !strings.Contains(stdout, "APK runtime closure OK") {
		t.Errorf("missing success message: %s", stdout)
	}
}

func TestMissingRequiredDexClassFails(t *testing.T) {
	temp := t.TempDir()
	apk := filepath.Join(temp, "app.apk")
	jar := filepath.Join(temp, "app_deploy.jar")
	writeZip(t, apk, map[string][]byte{"classes.dex": buildDex(requiredDexClasses[:2])})
	writeZip(t, jar, map[string][]byte{"com/example/Foo.class": buildClass("com/example/Foo", nil)})

	code, _, stderr := runClosure(t, apk, jar)
	if code == 0 {
		t.Fatal("missing DEX class accepted")
	}
	for _, descriptor := range requiredDexClasses[2:] {
		if !strings.Contains(stderr, descriptor) {
			t.Errorf("failure does not name %s: %s", descriptor, stderr)
		}
	}
}

func TestDexWithoutClassesFails(t *testing.T) {
	temp := t.TempDir()
	apk := filepath.Join(temp, "app.apk")
	jar := filepath.Join(temp, "app_deploy.jar")
	writeZip(t, apk, map[string][]byte{"resources.arsc": []byte("x")})
	writeZip(t, jar, map[string][]byte{"com/example/Foo.class": buildClass("com/example/Foo", nil)})

	code, _, stderr := runClosure(t, apk, jar)
	if code == 0 || !strings.Contains(stderr, "no classes*.dex entries found") {
		t.Fatalf("exit %d: %s", code, stderr)
	}
}

func TestUnparseableDexFails(t *testing.T) {
	temp := t.TempDir()
	apk := filepath.Join(temp, "app.apk")
	jar := filepath.Join(temp, "app_deploy.jar")
	writeZip(t, apk, map[string][]byte{"classes.dex": []byte("not a dex")})
	writeZip(t, jar, map[string][]byte{"com/example/Foo.class": buildClass("com/example/Foo", nil)})

	code, _, stderr := runClosure(t, apk, jar)
	if code == 0 || !strings.Contains(stderr, "cannot parse DEX class_defs: not a DEX file") {
		t.Fatalf("exit %d: %s", code, stderr)
	}
}

func TestSelfTestRejectsReferencedButUndefinedClass(t *testing.T) {
	temp := t.TempDir()
	apk := filepath.Join(temp, "app.apk")
	writeZip(t, apk, map[string][]byte{"classes.dex": buildDex(requiredDexClasses)})

	code, stdout, stderr := runClosure(t, "--self-test", apk)
	if code != 0 {
		t.Fatalf("exit %d: %s%s", code, stdout, stderr)
	}
	if !strings.Contains(stdout, "DEX definition self-test OK") {
		t.Errorf("missing self-test message: %s", stdout)
	}
}

func TestSelfTestWithoutRequiredClassFails(t *testing.T) {
	temp := t.TempDir()
	apk := filepath.Join(temp, "app.apk")
	writeZip(t, apk, map[string][]byte{"classes.dex": buildDex(requiredDexClasses[1:])})

	code, _, stderr := runClosure(t, "--self-test", apk)
	if code == 0 || !strings.Contains(stderr, "required class") {
		t.Fatalf("exit %d: %s", code, stderr)
	}
}

func TestDeployJarMissingReferenceFails(t *testing.T) {
	temp := t.TempDir()
	apk := filepath.Join(temp, "app.apk")
	jar := filepath.Join(temp, "app_deploy.jar")
	writeZip(t, apk, map[string][]byte{"classes.dex": buildDex(requiredDexClasses)})
	writeZip(t, jar, map[string][]byte{
		"com/example/Foo.class": buildClass("com/example/Foo", []string{"com/example/Bar"}),
	})

	code, _, stderr := runClosure(t, apk, jar)
	if code == 0 || !strings.Contains(stderr, "runtime closure is missing com.example.Bar") {
		t.Fatalf("exit %d: %s", code, stderr)
	}
}

func TestPlatformAndOptionalReferencesAreAllowed(t *testing.T) {
	temp := t.TempDir()
	apk := filepath.Join(temp, "app.apk")
	jar := filepath.Join(temp, "app_deploy.jar")
	writeZip(t, apk, map[string][]byte{"classes.dex": buildDex(requiredDexClasses)})
	writeZip(t, jar, map[string][]byte{
		"com/example/Foo.class": buildClass("com/example/Foo", []string{
			"java/lang/String",
			"org/bouncycastle/Foo",
			"com/google/firebase/components/Component$ComponentType",
		}),
	})

	code, stdout, stderr := runClosure(t, apk, jar)
	if code != 0 {
		t.Fatalf("exit %d: %s%s", code, stdout, stderr)
	}
}

func TestUnparseableClassFails(t *testing.T) {
	temp := t.TempDir()
	apk := filepath.Join(temp, "app.apk")
	jar := filepath.Join(temp, "app_deploy.jar")
	writeZip(t, apk, map[string][]byte{"classes.dex": buildDex(requiredDexClasses)})
	corrupt := []byte{0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52, 0, 2, 99}
	writeZip(t, jar, map[string][]byte{"com/example/Bad.class": corrupt})

	code, _, stderr := runClosure(t, apk, jar)
	if code == 0 || !strings.Contains(stderr, "cannot parse class in") {
		t.Fatalf("exit %d: %s", code, stderr)
	}
}

func TestUsageExitsTwo(t *testing.T) {
	code, _, stderr := runClosure(t, "only-one-argument")
	if code != 2 || !strings.Contains(stderr, "usage:") {
		t.Fatalf("exit %d: %s", code, stderr)
	}
}
