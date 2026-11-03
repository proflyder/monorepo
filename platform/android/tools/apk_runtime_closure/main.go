// Command apk_runtime_closure is the APK-level runtime closure gate for the
// Android client.
//
// This test inspects build artifacts rather than the JVM test classpath, because
// the Robolectric classpath contains Guava (as a test dependency) and therefore
// cannot detect a missing runtime ListenableFuture in the APK.
//
// Two independent checks run:
//
//  1. DEX definition check: the packaged classes*.dex must *define* the classes
//     that androidx.profileinstaller / androidx.concurrent.futures need at
//     runtime. Parsing DEX class_defs proves a definition; searching the string
//     table does not, because a missing class is still referenced by name there.
//  2. Closure check: every class referenced from the deploy jar (the exact set of
//     classes D8 receives) must be defined in that jar or be a platform/optional
//     class. This catches *any* missing transitive runtime dependency, not only
//     the one that caused the ProfileInstaller crash.
//
// The --self-test mode mutates a copy of the real DEX so that a required
// descriptor remains referenced by the string table but loses its class_def.
// The definition check must reject it. This runs as part of the Bazel test so
// the gate cannot silently degrade into a reference-only check.
package main

import (
	"archive/zip"
	"bytes"
	"encoding/binary"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

var requiredDexClasses = []string{
	"Lcom/google/common/util/concurrent/ListenableFuture;",
	"Landroidx/concurrent/futures/AbstractResolvableFuture;",
	"Landroidx/concurrent/futures/CallbackToFutureAdapter;",
	"Landroidx/profileinstaller/ProfileInstaller;",
}

var platformPrefixes = []string{
	"java.",
	"javax.",
	"jdk.",
	"sun.",
	"com.sun.",
	"android.",
	"dalvik.",
	"org.xml.",
	"org.xmlpull.",
	"org.w3c.",
	"org.json.",
	"org.apache.",
	"libcore.",
}

var optionalPrefixes = []string{
	"com.android.tools.r8.annotations.",
	"com.google.auto.value.",
	"com.google.errorprone.annotations.",
	"com.google.j2objc.annotations.",
	"org.checkerframework.",
	"org.bouncycastle.",
	"org.conscrypt.",
	"org.openjsse.",
}

var optionalClasses = map[string]bool{
	"com.google.firebase.components.Component$ComponentType": true,
	"com.google.firebase.components.Component$Instantiation": true,
}

var dexNamePattern = regexp.MustCompile(`^classes\d*\.dex$`)
var descriptorClassPattern = regexp.MustCompile(`L([A-Za-z0-9_$/]+);`)

func readU4(data []byte, offset int) int {
	if offset < 0 || offset+4 > len(data) {
		return 0
	}
	return int(binary.LittleEndian.Uint32(data[offset : offset+4]))
}

func readU2BE(data []byte, offset int) int {
	if offset < 0 || offset+2 > len(data) {
		return 0
	}
	return int(binary.BigEndian.Uint16(data[offset : offset+2]))
}

func readUleb128(data []byte, offset int) (int, int, error) {
	result := 0
	shift := 0
	for {
		if offset >= len(data) {
			return 0, offset, fmt.Errorf("unexpected end of DEX")
		}
		b := data[offset]
		offset++
		result |= int(b&0x7F) << shift
		if b&0x80 == 0 {
			return result, offset, nil
		}
		shift += 7
	}
}

func decodeUTF8(raw []byte) string {
	return strings.ToValidUTF8(string(raw), "\uFFFD")
}

func dexStringAt(data []byte, stringIDsSize, stringIDsOff, stringID int) (string, error) {
	if stringID >= stringIDsSize {
		return "", fmt.Errorf("string id out of range")
	}
	dataOff := readU4(data, stringIDsOff+4*stringID)
	_, cursor, err := readUleb128(data, dataOff)
	if err != nil {
		return "", err
	}
	end := bytes.IndexByte(data[cursor:], 0)
	if end < 0 {
		return "", fmt.Errorf("subsection not found")
	}
	return decodeUTF8(data[cursor : cursor+end]), nil
}

func dexDefinedTypes(data []byte) (map[string]bool, error) {
	if len(data) < 4 || string(data[:4]) != "dex\n" {
		return nil, fmt.Errorf("not a DEX file")
	}
	stringIDsSize := readU4(data, 0x38)
	stringIDsOff := readU4(data, 0x3C)
	typeIDsSize := readU4(data, 0x40)
	typeIDsOff := readU4(data, 0x44)
	classDefsSize := readU4(data, 0x60)
	classDefsOff := readU4(data, 0x64)
	if typeIDsOff+4*typeIDsSize > len(data) || classDefsOff+32*classDefsSize > len(data) {
		return nil, fmt.Errorf("DEX section offsets are out of bounds")
	}

	descriptor := func(typeID int) (string, error) {
		if typeID >= typeIDsSize {
			return "", fmt.Errorf("type id out of range")
		}
		return dexStringAt(data, stringIDsSize, stringIDsOff, readU4(data, typeIDsOff+4*typeID))
	}

	defined := map[string]bool{}
	for index := 0; index < classDefsSize; index++ {
		classIdx := readU4(data, classDefsOff+32*index)
		name, err := descriptor(classIdx)
		if err != nil {
			return nil, err
		}
		defined[name] = true
	}
	return defined, nil
}

// dexClassDefOffsets maps each defined descriptor to the offset of its
// class_def entry, preserving class_def order.
func dexClassDefOffsets(data []byte) (map[string]int, []string) {
	stringIDsOff := readU4(data, 0x3C)
	stringIDsSize := readU4(data, 0x38)
	typeIDsOff := readU4(data, 0x44)
	classDefsSize := readU4(data, 0x60)
	classDefsOff := readU4(data, 0x64)

	result := map[string]int{}
	var order []string
	for index := 0; index < classDefsSize; index++ {
		entry := classDefsOff + 32*index
		classIdx := readU4(data, entry)
		descriptorIdx := readU4(data, typeIDsOff+4*classIdx)
		name, err := dexStringAt(data, stringIDsSize, stringIDsOff, descriptorIdx)
		if err != nil {
			continue
		}
		if _, exists := result[name]; !exists {
			result[name] = entry
			order = append(order, name)
		}
	}
	return result, order
}

type dexEntry struct {
	name string
	data []byte
}

func apkDexBytes(apk string) ([]dexEntry, error) {
	reader, err := zip.OpenReader(apk)
	if err != nil {
		return nil, err
	}
	defer reader.Close()

	var names []string
	byName := map[string]*zip.File{}
	for _, file := range reader.File {
		if dexNamePattern.MatchString(file.Name) {
			names = append(names, file.Name)
			byName[file.Name] = file
		}
	}
	sort.Strings(names)

	var entries []dexEntry
	for _, name := range names {
		payload, err := readZipFile(byName[name])
		if err != nil {
			return nil, err
		}
		entries = append(entries, dexEntry{name: name, data: payload})
	}
	return entries, nil
}

func readZipFile(file *zip.File) ([]byte, error) {
	handle, err := file.Open()
	if err != nil {
		return nil, err
	}
	defer handle.Close()
	return io.ReadAll(handle)
}

func checkDex(apk string) ([]string, error) {
	dexes, err := apkDexBytes(apk)
	if err != nil {
		return nil, err
	}
	if len(dexes) == 0 {
		return []string{fmt.Sprintf("%s: no classes*.dex entries found", apk)}, nil
	}
	defined := map[string]bool{}
	for _, dex := range dexes {
		types, err := dexDefinedTypes(dex.data)
		if err != nil {
			return []string{fmt.Sprintf("%s:%s: cannot parse DEX class_defs: %s", apk, dex.name, err)}, nil
		}
		for name := range types {
			defined[name] = true
		}
	}
	var failures []string
	for _, descriptor := range requiredDexClasses {
		if !defined[descriptor] {
			failures = append(failures, fmt.Sprintf("DEX in %s does not define %s", apk, descriptor))
		}
	}
	return failures, nil
}

func selfTest(apk string) ([]string, error) {
	var failures []string
	dexes, err := apkDexBytes(apk)
	if err != nil {
		return nil, err
	}
	if len(dexes) == 0 {
		return []string{fmt.Sprintf("%s: no classes*.dex entries found for self-test", apk)}, nil
	}
	descriptor := requiredDexClasses[0]
	for _, dex := range dexes {
		offsets, order := dexClassDefOffsets(dex.data)
		entry, present := offsets[descriptor]
		if !present {
			continue
		}
		target := ""
		for _, candidate := range order {
			if candidate != descriptor {
				target = candidate
				break
			}
		}
		if target == "" {
			return []string{fmt.Sprintf("%s:%s: self-test needs a second defined class", apk, dex.name)}, nil
		}
		typeIDsOff := readU4(dex.data, 0x44)
		stringIDsOff := readU4(dex.data, 0x3C)
		stringIDsSize := readU4(dex.data, 0x38)
		typeIDsSize := readU4(dex.data, 0x40)

		targetTypeIdx := -1
		for typeIdx := 0; typeIdx < typeIDsSize; typeIdx++ {
			stringID := readU4(dex.data, typeIDsOff+4*typeIdx)
			name, err := dexStringAt(dex.data, stringIDsSize, stringIDsOff, stringID)
			if err != nil {
				return nil, err
			}
			if name == target {
				targetTypeIdx = typeIdx
				break
			}
		}
		if targetTypeIdx < 0 {
			return []string{fmt.Sprintf("%s:%s: self-test could not find target type", apk, dex.name)}, nil
		}
		mutated := append([]byte(nil), dex.data...)
		binary.LittleEndian.PutUint32(mutated[entry:entry+4], uint32(targetTypeIdx))
		if !bytes.Contains(mutated, []byte(descriptor)) {
			failures = append(failures, fmt.Sprintf("self-test setup: %s is no longer referenced in %s", descriptor, dex.name))
			continue
		}
		stillDefined, err := dexDefinedTypes(mutated)
		if err != nil {
			failures = append(failures, fmt.Sprintf("self-test could not parse mutated DEX: %s", err))
			continue
		}
		if stillDefined[descriptor] {
			failures = append(failures, fmt.Sprintf("self-test: mutated DEX still defines %s", descriptor))
			continue
		}

		mutatedAPK, err := writeMutatedAPK(apk, dex.name, mutated)
		if err != nil {
			return nil, err
		}
		gateFailures, err := checkDex(mutatedAPK)
		os.RemoveAll(filepath.Dir(mutatedAPK))
		if err != nil {
			return nil, err
		}
		found := false
		for _, failure := range gateFailures {
			if strings.Contains(failure, descriptor) && strings.Contains(failure, "does not define") {
				found = true
				break
			}
		}
		if !found {
			failures = append(failures, fmt.Sprintf(
				"self-test: check_dex accepted referenced-but-undefined %s: %s",
				descriptor, strings.Join(gateFailures, ", "),
			))
		}
		return failures, nil
	}
	failures = append(failures, fmt.Sprintf("self-test: required class %s not found in %s", descriptor, apk))
	return failures, nil
}

func writeMutatedAPK(source, dexName string, payload []byte) (string, error) {
	tempDir, err := os.MkdirTemp("", "apk-closure-")
	if err != nil {
		return "", err
	}
	target := filepath.Join(tempDir, "mutated.apk")

	reader, err := zip.OpenReader(source)
	if err != nil {
		return "", err
	}
	defer reader.Close()
	file, err := os.Create(target)
	if err != nil {
		return "", err
	}
	defer file.Close()
	writer := zip.NewWriter(file)
	for _, entry := range reader.File {
		contents := payload
		if entry.Name != dexName {
			contents, err = readZipFile(entry)
			if err != nil {
				return "", err
			}
		}
		header := entry.FileHeader
		header.CRC32 = 0
		header.CompressedSize = 0
		header.UncompressedSize = 0
		header.CompressedSize64 = 0
		header.UncompressedSize64 = 0
		destination, err := writer.CreateHeader(&header)
		if err != nil {
			return "", err
		}
		if _, err := destination.Write(contents); err != nil {
			return "", err
		}
	}
	if err := writer.Close(); err != nil {
		return "", err
	}
	return target, nil
}

func constantPoolClasses(data []byte) (map[string]bool, map[string]bool, error) {
	classes := map[string]bool{}
	descriptors := map[string]bool{}
	if len(data) < 4 || !bytes.Equal(data[:4], []byte{0xCA, 0xFE, 0xBA, 0xBE}) {
		return classes, descriptors, nil
	}
	count := readU2BE(data, 8)
	offset := 10
	utf8 := map[int]string{}
	var classNameIndices []int
	var descriptorIndices []int
	index := 1
	for index < count {
		if offset >= len(data) {
			return nil, nil, fmt.Errorf("unexpected end of class file")
		}
		tag := data[offset]
		offset++
		switch tag {
		case 1: // CONSTANT_Utf8
			length := readU2BE(data, offset)
			offset += 2
			end := offset + length
			if end > len(data) {
				end = len(data)
			}
			raw := data[offset:end]
			offset += length
			utf8[index] = decodeUTF8(raw)
		case 3, 4: // Integer, Float
			offset += 4
		case 5, 6: // Long, Double (take two slots)
			offset += 8
			index++
		case 7: // Class
			classNameIndices = append(classNameIndices, readU2BE(data, offset))
			offset += 2
		case 8: // String
			offset += 2
		case 9, 10, 11: // Fieldref, Methodref, InterfaceMethodref
			offset += 4
		case 12: // NameAndType
			offset += 2
			descriptorIndices = append(descriptorIndices, readU2BE(data, offset))
			offset += 2
		case 15: // MethodHandle
			offset += 3
		case 16: // MethodType
			offset += 2
		case 17, 18: // Dynamic, InvokeDynamic
			offset += 4
		case 19, 20: // Module, Package
			offset += 2
		default:
			return nil, nil, fmt.Errorf("unknown constant pool tag %d at %d", tag, offset)
		}
		index++
	}
	for _, index := range classNameIndices {
		if value, ok := utf8[index]; ok {
			classes[value] = true
		}
	}
	for _, index := range descriptorIndices {
		if value, ok := utf8[index]; ok {
			descriptors[value] = true
		}
	}
	return classes, descriptors, nil
}

func normalise(name string) string {
	name = strings.ReplaceAll(name, "/", ".")
	if strings.HasPrefix(name, "[") {
		name = strings.TrimLeft(name, "[")
		if !strings.HasPrefix(name, "L") {
			return ""
		}
		if strings.HasSuffix(name, ";") {
			name = name[1 : len(name)-1]
		} else {
			name = name[1:]
		}
	}
	if strings.HasPrefix(name, "L") && strings.HasSuffix(name, ";") {
		name = name[1 : len(name)-1]
	}
	if len(name) <= 1 || !strings.Contains(name, ".") {
		return ""
	}
	return name
}

func allowed(name string) bool {
	if optionalClasses[name] {
		return true
	}
	for _, prefix := range platformPrefixes {
		if strings.HasPrefix(name, prefix) {
			return true
		}
	}
	for _, prefix := range optionalPrefixes {
		if strings.HasPrefix(name, prefix) {
			return true
		}
	}
	return false
}

func checkDeployJar(deployJar string) ([]string, error) {
	reader, err := zip.OpenReader(deployJar)
	if err != nil {
		return nil, err
	}
	defer reader.Close()

	defined := map[string]bool{}
	var payloads [][]byte
	for _, file := range reader.File {
		name := file.Name
		if !strings.HasSuffix(name, ".class") || strings.HasPrefix(name, "META-INF/") {
			continue
		}
		defined[normalise(strings.TrimSuffix(name, ".class"))] = true
		payload, err := readZipFile(file)
		if err != nil {
			return nil, err
		}
		payloads = append(payloads, payload)
	}

	referenced := map[string]bool{}
	for _, payload := range payloads {
		classes, descriptors, err := constantPoolClasses(payload)
		if err != nil {
			return []string{fmt.Sprintf("cannot parse class in %s: %s", deployJar, err)}, nil
		}
		for name := range classes {
			referenced[normalise(name)] = true
		}
		for descriptor := range descriptors {
			for _, match := range descriptorClassPattern.FindAllStringSubmatch(descriptor, -1) {
				referenced[normalise(match[1])] = true
			}
		}
	}

	var missing []string
	for name := range referenced {
		if name != "" && !allowed(name) && !defined[name] {
			missing = append(missing, name)
		}
	}
	sort.Strings(missing)
	failures := make([]string, 0, len(missing))
	for _, name := range missing {
		failures = append(failures, fmt.Sprintf("runtime closure is missing %s", name))
	}
	return failures, nil
}

func run(args []string, stdout, stderr io.Writer) int {
	if len(args) == 2 && args[0] == "--self-test" {
		apk := filepath.Clean(args[1])
		failures, err := selfTest(apk)
		if err != nil {
			fmt.Fprintln(stderr, err)
			return 1
		}
		if len(failures) > 0 {
			fmt.Fprintln(stderr, "DEX definition self-test failed:")
			for _, failure := range failures {
				fmt.Fprintf(stderr, "  - %s\n", failure)
			}
			return 1
		}
		fmt.Fprintln(stdout, "DEX definition self-test OK: referenced-but-undefined class is rejected.")
		return 0
	}
	if len(args) != 2 {
		fmt.Fprintln(stderr, "usage: apk_runtime_closure <app.apk> <app_deploy.jar>")
		return 2
	}
	apk := filepath.Clean(args[0])
	deployJar := filepath.Clean(args[1])
	dexFails, err := checkDex(apk)
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}
	jarFails, err := checkDeployJar(deployJar)
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}
	failures := append(dexFails, jarFails...)
	if len(failures) > 0 {
		fmt.Fprintln(stderr, "APK runtime closure check failed:")
		for _, failure := range failures {
			fmt.Fprintf(stderr, "  - %s\n", failure)
		}
		return 1
	}
	fmt.Fprintln(stdout, "APK runtime closure OK: required classes are defined in DEX, "+
		"no unresolved non-platform references in the deploy jar.")
	return 0
}

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}
