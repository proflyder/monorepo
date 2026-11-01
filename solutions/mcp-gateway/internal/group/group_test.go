package group

import (
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"
)

type fakeGroup struct {
	name     string
	register int
}

func (g *fakeGroup) Name() string { return g.name }
func (g *fakeGroup) Register(*mcp.Server) {
	g.register++
}

func TestRegistrySelect(t *testing.T) {
	read := &fakeGroup{name: "infra.read"}
	fix := &fakeGroup{name: "infra.fix"}
	registry := NewRegistry(read, fix)

	if got := registry.Names(); len(got) != 2 || got[0] != "infra.fix" || got[1] != "infra.read" {
		t.Fatalf("Names = %v", got)
	}

	selected, err := registry.Select([]string{"infra.read", " infra.read ", "infra.fix"})
	if err != nil {
		t.Fatalf("Select: %v", err)
	}
	if len(selected) != 2 {
		t.Fatalf("selected = %d, want 2", len(selected))
	}
	if names := EnabledNames(selected); names[0] != "infra.read" || names[1] != "infra.fix" {
		t.Fatalf("EnabledNames = %v", names)
	}

	if _, err := registry.Select([]string{"infra.read", "infra.unknown"}); err == nil {
		t.Fatal("Select accepted an unknown group")
	}
	if _, err := registry.Select(nil); err == nil {
		t.Fatal("Select accepted an empty list")
	}
	if _, err := registry.Select([]string{" , "}); err == nil {
		t.Fatal("Select accepted only empties")
	}
}

func TestRegistryRegisterAll(t *testing.T) {
	first := &fakeGroup{name: "a"}
	second := &fakeGroup{name: "b"}
	registry := NewRegistry(first, second)
	server := mcp.NewServer(&mcp.Implementation{Name: "test", Version: "0"}, nil)
	registry.RegisterAll(server, []Group{first, second})
	if first.register != 1 || second.register != 1 {
		t.Fatalf("register counts = %d, %d", first.register, second.register)
	}
}

func TestRegistryRejectsDuplicate(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Fatal("NewRegistry accepted duplicate names")
		}
	}()
	NewRegistry(&fakeGroup{name: "a"}, &fakeGroup{name: "a"})
}
