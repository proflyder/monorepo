package bb

import (
	"errors"
	"testing"
)

func testTargets() Targets {
	return Targets{Projects: []Target{
		{
			ProjectID: "proj_personal",
			Name:      "Station Lab",
			Profile:   "personal",
			Providers: []Provider{{
				ProviderID: "claude-code",
				Models: []Model{{
					Model:                 "claude-sonnet",
					DefaultReasoningLevel: "medium",
					ReasoningLevels:       []string{"low", "medium", "high"},
				}},
			}},
		},
		{
			ProjectID: "proj_work",
			Name:      "Work",
			Profile:   "work",
			Providers: []Provider{{
				ProviderID: "codex",
				Models:     []Model{{Model: "gpt", DefaultReasoningLevel: "low"}},
			}},
		},
	}}
}

func testResolver(t *testing.T) *Resolver {
	t.Helper()
	resolver, err := ParseTargets([]byte(`targets:
  - project: proj_personal
    aliases: ["стан лэб", "station lab"]
    provider: claude-code
    title_prefix: "Голос"
`))
	if err != nil {
		t.Fatalf("ParseTargets: %v", err)
	}
	return resolver
}

func TestResolvePersonalTarget(t *testing.T) {
	resolver := testResolver(t)
	got, err := resolver.Resolve(testTargets(), "Стан лэб", "", "")
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if got.ProjectID != "proj_personal" || got.ProviderID != "claude-code" || got.Model != "claude-sonnet" {
		t.Fatalf("resolution = %+v", got)
	}
	if got.Effort != "medium" || got.TitlePrefix != "Голос" {
		t.Fatalf("resolution = %+v", got)
	}
}

func TestResolveUnknownAlias(t *testing.T) {
	resolver := testResolver(t)
	if _, err := resolver.Resolve(testTargets(), "станция", "", ""); !errors.Is(err, ErrTargetNotFound) {
		t.Fatalf("err = %v", err)
	}
}

// A work project is invisible even when the alias exists in the config: the
// resolver refuses it exactly like an unknown name.
func TestResolveRefusesWorkProject(t *testing.T) {
	resolver, err := ParseTargets([]byte(`targets:
  - project: proj_work
    aliases: ["работа"]
  - project: proj_unknown
    aliases: ["неизвестный"]
`))
	if err != nil {
		t.Fatalf("ParseTargets: %v", err)
	}
	if _, err := resolver.Resolve(testTargets(), "работа", "", ""); !errors.Is(err, ErrTargetNotFound) {
		t.Fatalf("work target err = %v", err)
	}
	if _, err := resolver.Resolve(testTargets(), "work", "", ""); !errors.Is(err, ErrTargetNotFound) {
		t.Fatalf("work name err = %v", err)
	}
	targets := testTargets()
	targets.Projects = append(targets.Projects, Target{
		ProjectID: "proj_unknown",
		Name:      "Unknown",
		Profile:   "",
	})
	if _, err := resolver.Resolve(targets, "неизвестный", "", ""); !errors.Is(err, ErrTargetNotFound) {
		t.Fatalf("empty-profile target err = %v", err)
	}
}

func TestResolveOverridesAreValidatedByPluginData(t *testing.T) {
	resolver := testResolver(t)
	got, err := resolver.Resolve(testTargets(), "стан лэб", "claude-sonnet", "high")
	if err != nil || got.Effort != "high" {
		t.Fatalf("override = %+v, %v", got, err)
	}
	if _, err := resolver.Resolve(testTargets(), "стан лэб", "not-a-model", ""); !errors.Is(err, ErrModelNotFound) {
		t.Fatalf("model err = %v", err)
	}
	if _, err := resolver.Resolve(testTargets(), "стан лэб", "", "extreme"); !errors.Is(err, ErrEffortNotSupported) {
		t.Fatalf("effort err = %v", err)
	}
}

func TestParseTargetsRejectsDuplicateAliases(t *testing.T) {
	_, err := ParseTargets([]byte(`targets:
  - project: one
    aliases: ["стан"]
  - project: two
    aliases: ["стан"]
`))
	if err == nil {
		t.Fatal("duplicate alias accepted")
	}
}
