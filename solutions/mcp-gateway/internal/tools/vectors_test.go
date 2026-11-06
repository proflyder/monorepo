package tools

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/catalog"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/checks"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/golden"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/kubernetes"
)

func TestSpeechVectors(t *testing.T) {
	path := golden.RelativePath(t, "solutions/mcp-gateway/testdata/vectors/speech.json")
	golden.Run(t, path, func(t *testing.T, _ golden.File, input json.RawMessage) (any, error) {
		var in struct {
			Text string `json:"text"`
		}
		if err := json.Unmarshal(input, &in); err != nil {
			return nil, err
		}
		return Speech(in.Text), nil
	})
}

type reportVectorInput struct {
	Now       string                `json:"now"`
	Timezone  string                `json:"timezone"`
	Catalog   string                `json:"catalog"`
	Workloads []kubernetes.Workload `json:"workloads"`
	Pods      []kubernetes.Pod      `json:"pods"`
	Checks    []checks.Result       `json:"checks"`
	Verbose   bool                  `json:"verbose"`
}

func TestReportVectors(t *testing.T) {
	path := golden.RelativePath(t, "solutions/mcp-gateway/testdata/vectors/report_inputs.json")
	golden.Run(t, path, func(t *testing.T, _ golden.File, input json.RawMessage) (any, error) {
		var in reportVectorInput
		if err := json.Unmarshal(input, &in); err != nil {
			return nil, err
		}
		cat, err := catalog.Parse([]byte(in.Catalog))
		if err != nil {
			return nil, err
		}
		now, err := time.Parse(time.RFC3339, in.Now)
		if err != nil {
			return nil, err
		}
		location, err := time.LoadLocation(in.Timezone)
		if err != nil {
			return nil, err
		}
		d := testDeps(t)
		d.Catalog = cat
		d.Now = func() time.Time { return now }
		d.Location = location
		kube := d.Kube.(*fakeKube)
		for _, workload := range in.Workloads {
			kube.workloads[kubeKey(workload.Namespace, workload.Kind, workload.Name)] = workload
		}
		kube.pods["home"] = in.Pods
		byService := map[string][]checks.Result{}
		for _, check := range in.Checks {
			byService[check.Service] = append(byService[check.Service], check)
		}
		d.Checks = &fakeChecks{byService: byService}
		_, out, err := d.infraReport(context.Background(), nil, infraReportInput{Verbose: in.Verbose})
		if err != nil {
			return nil, err
		}
		result := out.(Result)
		detail := result.Details.(infraReportDetail)
		return map[string]any{
			"speech": result.Speech,
			"text":   detail.Text,
		}, nil
	})
}
