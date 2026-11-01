package tools

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/alertmanager"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/catalog"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/checks"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/kubernetes"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/redact"
)

// ReadGroup is the infra.read tool group.
type ReadGroup struct{ Deps *Deps }

// Name implements group.Group.
func (g *ReadGroup) Name() string { return GroupRead }

// Register implements group.Group.
func (g *ReadGroup) Register(r *mcp.Server) {
	d := g.Deps
	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_status",
		Description: "One aggregate infra health check: firing alerts, catalog health signals, Kubernetes workload availability and recent pod restarts. Returns a short Russian speech verdict plus structured details.",
		Annotations: readOnly(),
	}, instrument(d, "infra_status", d.infraStatus))

	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_service",
		Description: "Detail for one catalog service resolved by spoken name (case and ё-insensitive): workload status, pods, restarts, recent events, Prometheus signals and related alerts.",
		Annotations: readOnly(),
	}, instrument(d, "infra_service", d.infraService))

	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_alert",
		Description: "Explain one active alert by Alertmanager fingerprint (full or a prefix of at least 6 characters) or by alert name. Returns its labels and annotations plus the matching catalog service detail, so a follow-up question about an alert can be answered.",
		Annotations: readOnly(),
	}, instrument(d, "infra_alert", d.infraAlert))

	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_logs",
		Description: "Recent logs for any catalog service: Loki when the service is ingested there, otherwise Kubernetes pods/log. Secret-like values are redacted before returning.",
		Annotations: readOnly(),
	}, instrument(d, "infra_logs", d.infraLogs))

	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_query",
		Description: "Run one read-only instant PromQL query. Results are capped; this tool cannot change anything.",
		Annotations: readOnly(),
	}, instrument(d, "infra_query", d.infraQuery))

	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_overview",
		Description: "Topology overview and the service catalog (spoken aliases, workloads, restart policy) for model context.",
		Annotations: readOnly(),
	}, instrument(d, "infra_overview", d.infraOverview))

	mcp.AddTool(r, &mcp.Tool{
		Name: "infra_report",
		Description: "Full infrastructure report across every catalog service: status, Kubernetes replicas, restarts in the last hour, image digest, age, external TCP/HTTP/Home Assistant checks, versions and active alerts. " +
			"Returns a short Russian verdict plus a compact plain-text report suitable for Telegram. Read-only; use infra_send_report to deliver the text.",
		Annotations: readOnly(),
	}, instrument(d, "infra_report", d.infraReport))

	r.AddResource(&mcp.Resource{
		URI:         "infra://catalog",
		Name:        "infra catalog",
		Title:       "Infra catalog",
		Description: "Sanitized topology overview plus the full service catalog.",
		MIMEType:    "text/markdown",
	}, d.readCatalogResource)
}

func readOnly() *mcp.ToolAnnotations {
	openWorld := false
	return &mcp.ToolAnnotations{ReadOnlyHint: true, OpenWorldHint: &openWorld}
}

func destructive() *mcp.ToolAnnotations {
	openWorld := false
	destructiveUpdate := true
	return &mcp.ToolAnnotations{
		ReadOnlyHint:    false,
		DestructiveHint: &destructiveUpdate,
		OpenWorldHint:   &openWorld,
	}
}

// --- infra_status ---------------------------------------------------------

type alertDetail struct {
	Name        string            `json:"name"`
	Fingerprint string            `json:"fingerprint,omitempty"`
	Severity    string            `json:"severity,omitempty"`
	Summary     string            `json:"summary"`
	Description string            `json:"description,omitempty"`
	StartedAt   string            `json:"startedAt,omitempty"`
	Labels      map[string]string `json:"labels,omitempty"`
	Annotations map[string]string `json:"annotations,omitempty"`
}

type signalDetail struct {
	Service string  `json:"service"`
	Meaning string  `json:"meaning"`
	PromQL  string  `json:"promql"`
	Firing  bool    `json:"firing"`
	Value   float64 `json:"value"`
	Error   string  `json:"error,omitempty"`
}

type workloadDetail struct {
	Service   string `json:"service"`
	Namespace string `json:"namespace"`
	Kind      string `json:"kind"`
	Name      string `json:"name"`
	Desired   int64  `json:"desired"`
	Ready     int64  `json:"ready"`
	Available int64  `json:"available"`
	Healthy   bool   `json:"healthy"`
	Error     string `json:"error,omitempty"`
}

type podDetail struct {
	Service   string `json:"service"`
	Namespace string `json:"namespace"`
	Pod       string `json:"pod"`
	Container string `json:"container,omitempty"`
	Reason    string `json:"reason"`
	Restarts  int32  `json:"restarts"`
}

type statusDetail struct {
	Verdict      string        `json:"verdict"`
	CheckedAt    string        `json:"checkedAt"`
	ServiceCount int           `json:"serviceCount"`
	Degraded     bool          `json:"degraded"`
	Partial      bool          `json:"partial"`
	Alerts       []alertDetail `json:"alerts"`
	// IgnoredAlerts are active alerts the catalog marks as permanent
	// background noise (ignore_alerts). They never affect the verdict.
	IgnoredAlerts []alertDetail    `json:"ignoredAlerts"`
	Signals       []signalDetail   `json:"signals"`
	Workloads     []workloadDetail `json:"workloads"`
	Restarts      []podDetail      `json:"restarts"`
	// ExternalChecks are the typed catalog probes (TCP/HTTP/Home Assistant).
	// Skipped checks mean "not configured", never a failure.
	ExternalChecks []checks.Result `json:"externalChecks"`
	Errors         []string        `json:"errors,omitempty"`
}

func (d *Deps) infraStatus(ctx context.Context, _ *mcp.CallToolRequest, _ struct{}) (*mcp.CallToolResult, any, error) {
	cat := d.Catalog
	if cat == nil {
		return Failure("Каталог сервисов не загружен.", nil)
	}
	ctx, cancel := context.WithTimeout(ctx, d.statusTimeout())
	defer cancel()

	var mu sync.Mutex
	detail := statusDetail{
		CheckedAt:      d.now().UTC().Format(time.RFC3339),
		ServiceCount:   len(cat.Services),
		Alerts:         []alertDetail{},
		IgnoredAlerts:  []alertDetail{},
		Signals:        []signalDetail{},
		Workloads:      []workloadDetail{},
		Restarts:       []podDetail{},
		ExternalChecks: []checks.Result{},
	}
	addError := func(msg string) {
		mu.Lock()
		defer mu.Unlock()
		detail.Errors = append(detail.Errors, msg)
	}
	var wg sync.WaitGroup

	if d.Alerts != nil {
		wg.Add(1)
		go func() {
			defer wg.Done()
			callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
			defer cancel()
			alerts, err := d.Alerts.Active(callCtx, 20)
			if err != nil {
				addError("алерты: " + err.Error())
				return
			}
			items := make([]alertDetail, 0, len(alerts))
			ignored := make([]alertDetail, 0)
			for _, a := range alerts {
				item := alertDetail{
					Name:      a.Labels["alertname"],
					Severity:  a.Severity(),
					Summary:   a.Summary(),
					StartedAt: a.StartsAt.UTC().Format(time.RFC3339),
				}
				if cat.IgnoresAlert(item.Name) {
					ignored = append(ignored, item)
					continue
				}
				items = append(items, item)
			}
			mu.Lock()
			detail.Alerts = items
			detail.IgnoredAlerts = ignored
			mu.Unlock()
		}()
	} else {
		addError("алерты: Alertmanager не настроен")
	}

	if d.Checks != nil {
		wg.Add(1)
		go func() {
			defer wg.Done()
			results := d.Checks.RunAll(ctx, cat.Services)
			mu.Lock()
			detail.ExternalChecks = results
			mu.Unlock()
		}()
	}

	for _, service := range cat.Services {
		service := service
		for _, sig := range service.Signals {
			if !sig.Status {
				continue
			}
			sig := sig
			wg.Add(1)
			go func() {
				defer wg.Done()
				finding := signalDetail{Service: service.ID, Meaning: sig.Meaning, PromQL: sig.PromQL}
				if d.Prom == nil {
					finding.Error = "Prometheus не настроен"
					mu.Lock()
					detail.Signals = append(detail.Signals, finding)
					mu.Unlock()
					addError("сигналы: Prometheus не настроен")
					return
				}
				callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
				defer cancel()
				samples, err := d.Prom.Query(callCtx, sig.PromQL, 5)
				if err != nil {
					finding.Error = err.Error()
					addError("сигнал " + service.ID + ": " + err.Error())
				} else if len(samples) > 0 {
					finding.Firing = true
					finding.Value = samples[0].Value
				}
				mu.Lock()
				detail.Signals = append(detail.Signals, finding)
				mu.Unlock()
			}()
		}
		if service.Kind != catalog.ServiceK8s {
			continue
		}
		wg.Add(1)
		go func() {
			defer wg.Done()
			item := workloadDetail{Service: service.ID, Namespace: service.Namespace, Kind: service.Workload.Kind, Name: service.Workload.Name}
			if d.Kube == nil {
				item.Error = "Kubernetes API не настроен"
				addError("кластер: Kubernetes API не настроен")
				mu.Lock()
				detail.Workloads = append(detail.Workloads, item)
				mu.Unlock()
				return
			}
			callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
			defer cancel()
			w, err := d.Kube.GetWorkload(callCtx, service.Namespace, service.Workload.Kind, service.Workload.Name)
			if err != nil {
				item.Error = err.Error()
				addError("нагрузка " + service.ID + ": " + err.Error())
			} else {
				item.Desired, item.Ready, item.Available = w.Desired, w.Ready, w.Available
				item.Healthy = w.AvailableOK()
			}
			mu.Lock()
			detail.Workloads = append(detail.Workloads, item)
			mu.Unlock()
		}()
	}

	namespaces := namespacesOf(cat)
	if d.Kube != nil {
		for _, namespace := range namespaces {
			namespace := namespace
			wg.Add(1)
			go func() {
				defer wg.Done()
				callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
				defer cancel()
				pods, err := d.Kube.ListPods(callCtx, namespace, nil)
				if err != nil {
					addError("поды " + namespace + ": " + err.Error())
					return
				}
				cutoff := d.now().Add(-time.Hour)
				var findings []podDetail
				for _, pod := range pods {
					for _, cs := range pod.Containers {
						reason := ""
						if strings.Contains(cs.WaitingReason, "CrashLoopBackOff") {
							reason = cs.WaitingReason
						} else if cs.RestartCount > 0 && !cs.LastTerminatedAt.IsZero() && cs.LastTerminatedAt.After(cutoff) {
							reason = "перезапуск за последний час"
						}
						if reason == "" {
							continue
						}
						findings = append(findings, podDetail{
							Namespace: namespace,
							Pod:       pod.Name,
							Container: cs.Name,
							Reason:    reason,
							Restarts:  cs.RestartCount,
						})
					}
				}
				if len(findings) == 0 {
					return
				}
				mu.Lock()
				detail.Restarts = append(detail.Restarts, findings...)
				mu.Unlock()
			}()
		}
	}

	wg.Wait()

	sort.SliceStable(detail.Alerts, func(i, j int) bool { return detail.Alerts[i].Name < detail.Alerts[j].Name })
	sort.SliceStable(detail.IgnoredAlerts, func(i, j int) bool { return detail.IgnoredAlerts[i].Name < detail.IgnoredAlerts[j].Name })
	sort.SliceStable(detail.Signals, func(i, j int) bool {
		if detail.Signals[i].Service != detail.Signals[j].Service {
			return detail.Signals[i].Service < detail.Signals[j].Service
		}
		return detail.Signals[i].Meaning < detail.Signals[j].Meaning
	})
	sort.SliceStable(detail.Workloads, func(i, j int) bool { return detail.Workloads[i].Service < detail.Workloads[j].Service })
	sort.SliceStable(detail.Restarts, func(i, j int) bool { return detail.Restarts[i].Pod < detail.Restarts[j].Pod })

	issues := statusIssues(detail)
	detail.Degraded = len(issues) > 0
	detail.Partial = len(detail.Errors) > 0
	detail.Verdict = statusVerdict(detail, issues)
	return respond(Result{Speech: detail.Verdict, Details: detail})
}

func statusIssues(detail statusDetail) []string {
	var issues []string
	for _, a := range detail.Alerts {
		label := a.Summary
		if a.Severity != "" {
			label += " (" + a.Severity + ")"
		}
		issues = append(issues, label)
	}
	for _, s := range detail.Signals {
		if s.Firing {
			issues = append(issues, s.Meaning)
		}
	}
	for _, w := range detail.Workloads {
		if w.Error != "" || !w.Healthy {
			issues = append(issues, fmt.Sprintf("%s: готово %d из %d", w.Service, w.Ready, w.Desired))
		}
	}
	for _, p := range detail.Restarts {
		issues = append(issues, p.Pod+": "+p.Reason)
	}
	for _, check := range detail.ExternalChecks {
		if check.Skipped || check.OK {
			continue
		}
		issues = append(issues, check.Label+": "+check.Error)
	}
	return issues
}

func statusVerdict(detail statusDetail, issues []string) string {
	switch {
	case len(issues) == 0 && len(detail.Errors) == 0:
		return fmt.Sprintf("Всё в порядке: %d сервисов, алертов нет.", detail.ServiceCount)
	case len(issues) > 0:
		shown := issues
		if len(shown) > 2 {
			shown = shown[:2]
		}
		text := "Проблемы: " + strings.Join(shown, "; ")
		if len(issues) > len(shown) {
			text += fmt.Sprintf(" и ещё %d", len(issues)-len(shown))
		}
		return text + "."
	default:
		return "Проверка неполная: " + detail.Errors[0] + "."
	}
}

func namespacesOf(cat *catalog.Catalog) []string {
	seen := map[string]bool{}
	var out []string
	for _, s := range cat.K8sServices() {
		if !seen[s.Namespace] {
			seen[s.Namespace] = true
			out = append(out, s.Namespace)
		}
	}
	sort.Strings(out)
	return out
}

// --- infra_service --------------------------------------------------------

type serviceRef struct {
	ID             string `json:"id"`
	Kind           string `json:"kind"`
	Namespace      string `json:"namespace,omitempty"`
	WorkloadKind   string `json:"workloadKind,omitempty"`
	WorkloadName   string `json:"workloadName,omitempty"`
	RestartAllowed bool   `json:"restartAllowed"`
}

type serviceDetail struct {
	Service  serviceRef           `json:"service"`
	Workload *kubernetes.Workload `json:"workload,omitempty"`
	Pods     []kubernetes.Pod     `json:"pods,omitempty"`
	Events   []kubernetes.Event   `json:"events,omitempty"`
	Signals  []signalDetail       `json:"signals"`
	Alerts   []alertDetail        `json:"alerts"`
	Errors   []string             `json:"errors,omitempty"`
}

type serviceInput struct {
	Name string `json:"name" jsonschema:"spoken service name from the catalog"`
}

func (d *Deps) infraService(ctx context.Context, _ *mcp.CallToolRequest, in serviceInput) (*mcp.CallToolResult, any, error) {
	service, err := d.resolve(in.Name)
	if err != nil {
		return Failure("Не нашёл сервис "+strings.TrimSpace(in.Name)+".", map[string]string{"error": err.Error()})
	}
	detail := d.collectServiceDetail(ctx, service, nil)
	return respond(Result{Speech: serviceSpeech(DisplayName(service), service, detail), Details: detail})
}

// collectServiceDetail builds the workload/pods/events/signals/alerts view of
// one catalog service. alerts may be prefetched by the caller; nil means "fetch
// the active set now".
func (d *Deps) collectServiceDetail(ctx context.Context, service catalog.Service, alerts []alertmanager.Alert) serviceDetail {
	detail := serviceDetail{
		Service: serviceRef{
			ID:             service.ID,
			Kind:           service.Kind,
			Namespace:      service.Namespace,
			WorkloadKind:   service.Workload.Kind,
			WorkloadName:   service.Workload.Name,
			RestartAllowed: service.Restart.Allowed,
		},
		Signals: []signalDetail{},
		Alerts:  []alertDetail{},
	}

	if service.Kind == catalog.ServiceK8s && d.Kube == nil {
		detail.Errors = append(detail.Errors, "Kubernetes API не настроен")
	}
	if service.Kind == catalog.ServiceK8s && d.Kube != nil {
		callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
		w, err := d.Kube.GetWorkload(callCtx, service.Namespace, service.Workload.Kind, service.Workload.Name)
		if err != nil {
			detail.Errors = append(detail.Errors, err.Error())
		} else {
			detail.Workload = &w
			pods, err := d.Kube.ListPods(callCtx, service.Namespace, w.Selector)
			if err != nil {
				detail.Errors = append(detail.Errors, err.Error())
			}
			detail.Pods = pods
			events, err := d.Kube.ListEvents(callCtx, service.Namespace, service.Workload.Name, 10)
			if err != nil {
				detail.Errors = append(detail.Errors, err.Error())
			}
			detail.Events = events
		}
		cancel()
	}

	for _, sig := range service.Signals {
		sig := sig
		finding := signalDetail{Service: service.ID, Meaning: sig.Meaning, PromQL: sig.PromQL}
		if d.Prom == nil {
			finding.Error = "Prometheus не настроен"
		} else {
			callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
			samples, err := d.Prom.Query(callCtx, sig.PromQL, 5)
			cancel()
			if err != nil {
				finding.Error = err.Error()
			} else if len(samples) > 0 {
				finding.Firing = true
				finding.Value = samples[0].Value
			}
		}
		detail.Signals = append(detail.Signals, finding)
	}

	if alerts == nil && d.Alerts != nil {
		callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
		fetched, err := d.Alerts.Active(callCtx, 50)
		cancel()
		if err != nil {
			detail.Errors = append(detail.Errors, err.Error())
		}
		alerts = fetched
	}
	for _, a := range alerts {
		if alertMatches(a, service) {
			detail.Alerts = append(detail.Alerts, compactAlert(a))
		}
	}
	return detail
}

func compactAlert(a alertmanager.Alert) alertDetail {
	return alertDetail{
		Name:      a.Labels["alertname"],
		Severity:  a.Severity(),
		Summary:   a.Summary(),
		StartedAt: a.StartsAt.UTC().Format(time.RFC3339),
	}
}

func fullAlert(a alertmanager.Alert) alertDetail {
	detail := compactAlert(a)
	detail.Fingerprint = a.Fingerprint
	detail.Description = a.Annotations["description"]
	detail.Labels = a.Labels
	detail.Annotations = a.Annotations
	return detail
}

func serviceSpeech(display string, service catalog.Service, detail serviceDetail) string {
	if len(detail.Errors) > 0 {
		return display + ": не удалось получить состояние — " + detail.Errors[0]
	}
	var issues []string
	for _, s := range detail.Signals {
		if s.Firing {
			issues = append(issues, s.Meaning)
		}
	}
	for _, a := range detail.Alerts {
		issues = append(issues, a.Summary)
	}
	if service.Kind == catalog.ServiceK8s && detail.Workload != nil {
		w := *detail.Workload
		if !w.AvailableOK() {
			issues = append(issues, fmt.Sprintf("готовы %d из %d подов", w.Ready, w.Desired))
		}
		var notReady []string
		for _, pod := range detail.Pods {
			if !pod.Ready {
				notReady = append(notReady, pod.Name)
			}
		}
		if len(notReady) > 0 && len(issues) == 0 {
			issues = append(issues, "не готовы поды: "+strings.Join(notReady, ", "))
		}
	}
	if len(issues) == 0 {
		if service.Kind == catalog.ServiceK8s && detail.Workload != nil {
			w := *detail.Workload
			return fmt.Sprintf("%s работает: готовы %d из %d, подов %d.", display, w.Ready, w.Desired, len(detail.Pods))
		}
		return display + ": сигналы в норме."
	}
	if len(issues) > 2 {
		issues = issues[:2]
	}
	return display + ": " + strings.Join(issues, "; ") + "."
}

func alertMatches(a alertmanager.Alert, service catalog.Service) bool {
	for key, value := range a.Labels {
		v := strings.ToLower(value)
		switch key {
		case "namespace":
			if service.Namespace != "" && v == strings.ToLower(service.Namespace) {
				// Namespace alone is too broad: require another match below.
				continue
			}
		}
		if v == "" {
			continue
		}
		if v == strings.ToLower(service.ID) ||
			(service.Workload.Name != "" && strings.Contains(v, strings.ToLower(service.Workload.Name))) {
			return true
		}
	}
	return false
}

// --- infra_alert ----------------------------------------------------------

type alertInput struct {
	Ref string `json:"ref" jsonschema:"Alertmanager fingerprint (full or a prefix of at least 6 characters) or the alert name"`
}

type alertQueryDetail struct {
	Ref      string          `json:"ref"`
	Count    int             `json:"count"`
	Alerts   []alertDetail   `json:"alerts"`
	Services []serviceDetail `json:"services,omitempty"`
}

func (d *Deps) infraAlert(ctx context.Context, _ *mcp.CallToolRequest, in alertInput) (*mcp.CallToolResult, any, error) {
	ref := strings.TrimSpace(in.Ref)
	if ref == "" {
		return Failure("Нужен отпечаток или имя алерта.", nil)
	}
	if d.Alerts == nil {
		return Failure("Alertmanager не настроен.", nil)
	}
	callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
	alerts, err := d.Alerts.Active(callCtx, 100)
	cancel()
	if err != nil {
		return Failure("Не удалось получить алерты.", map[string]string{"error": err.Error()})
	}

	matches := matchAlerts(alerts, ref)
	detail := alertQueryDetail{Ref: ref, Alerts: []alertDetail{}}
	if len(matches) == 0 {
		return respond(Result{Speech: "Алерт не найден: " + ref + ".", Details: detail})
	}
	detail.Count = len(matches)
	const maxAlerts = 10
	for i, alert := range matches {
		if i >= maxAlerts {
			break
		}
		detail.Alerts = append(detail.Alerts, fullAlert(alert))
	}
	detail.Services = d.servicesForAlerts(ctx, matches, alerts)
	return respond(Result{Speech: alertSpeech(ref, matches, detail.Services), Details: detail})
}

// matchAlerts selects alerts by name (case-insensitive, exact) or by
// Alertmanager fingerprint prefix. A fingerprint prefix must be at least 6
// characters so a short accidental string cannot match half the alert list.
func matchAlerts(alerts []alertmanager.Alert, ref string) []alertmanager.Alert {
	norm := strings.ToLower(strings.TrimSpace(ref))
	if norm == "" {
		return nil
	}
	var out []alertmanager.Alert
	for _, alert := range alerts {
		name := strings.ToLower(strings.TrimSpace(alert.Labels["alertname"]))
		fingerprint := strings.ToLower(strings.TrimSpace(alert.Fingerprint))
		if name == norm || (len(norm) >= 6 && fingerprint != "" && strings.HasPrefix(fingerprint, norm)) {
			out = append(out, alert)
		}
	}
	return out
}

// alertSpeech summarizes the match. With several matches only the first one is
// named, keeping the spoken line short.
func alertSpeech(ref string, matches []alertmanager.Alert, services []serviceDetail) string {
	if len(matches) == 0 {
		return "Алерт не найден: " + ref + "."
	}
	first := matches[0]
	summary := strings.TrimSpace(first.Summary())
	if summary == "" {
		summary = ref
	}
	if len(matches) > 1 {
		return fmt.Sprintf("Нашёл %d алертов по %q; первый: %s.", len(matches), ref, summary)
	}
	text := "Алерт: " + summary
	if severity := first.Severity(); severity != "" {
		text += ", " + severity
	}
	if len(services) > 0 {
		service := services[0]
		text += ". Сервис " + service.Service.ID
		if service.Workload != nil {
			text += fmt.Sprintf(": готовы %d из %d", service.Workload.Ready, service.Workload.Desired)
		}
	} else {
		text += ". Сервис не определён"
	}
	return text + "."
}

// servicesForAlerts maps alert labels onto catalog entries. A specific label
// match (deployment, pod, endpoint, job, …) beats a bare namespace match;
// at most three services are inspected so one alert cannot trigger a fan-out.
func (d *Deps) servicesForAlerts(ctx context.Context, matches []alertmanager.Alert, alerts []alertmanager.Alert) []serviceDetail {
	if d.Catalog == nil {
		return nil
	}
	var specific, namespaceOnly []catalog.Service
	seen := map[string]bool{}
	for _, alert := range matches {
		for _, service := range d.Catalog.Services {
			if seen[service.ID] {
				continue
			}
			matched, specificMatch := alertServiceMatch(alert, service)
			if !matched {
				continue
			}
			seen[service.ID] = true
			if specificMatch {
				specific = append(specific, service)
			} else {
				namespaceOnly = append(namespaceOnly, service)
			}
		}
	}
	sort.SliceStable(specific, func(i, j int) bool { return specific[i].ID < specific[j].ID })
	sort.SliceStable(namespaceOnly, func(i, j int) bool { return namespaceOnly[i].ID < namespaceOnly[j].ID })
	ordered := append(specific, namespaceOnly...)

	const maxServices = 3
	if len(ordered) > maxServices {
		ordered = ordered[:maxServices]
	}
	out := make([]serviceDetail, 0, len(ordered))
	for _, service := range ordered {
		out = append(out, d.collectServiceDetail(ctx, service, alerts))
	}
	return out
}

// alertServiceMatch reports whether an alert label refers to the service, and
// whether that match is specific (not namespace-only).
func alertServiceMatch(alert alertmanager.Alert, service catalog.Service) (matched bool, specific bool) {
	namespaceMatch := false
	for key, value := range alert.Labels {
		normalized := catalog.Normalize(value)
		if normalized == "" {
			continue
		}
		if strings.ToLower(key) == "namespace" {
			if service.Namespace != "" && normalized == catalog.Normalize(service.Namespace) {
				namespaceMatch = true
			}
			continue
		}
		if serviceMatchesLabelValue(service, normalized) {
			return true, true
		}
	}
	return namespaceMatch, false
}

func serviceMatchesLabelValue(service catalog.Service, value string) bool {
	if value == catalog.Normalize(service.ID) {
		return true
	}
	if name := catalog.Normalize(service.Workload.Name); name != "" {
		// Pod and endpoint label values usually extend the workload name with a
		// suffix, for example "hermes-7d9f8c6b5-x2x4z".
		if value == name || strings.HasPrefix(value, name+"-") {
			return true
		}
	}
	for _, alias := range service.AliasIndex {
		if value == catalog.Normalize(alias) {
			return true
		}
	}
	return false
}

// --- infra_logs -----------------------------------------------------------

type logsInput struct {
	Name  string `json:"name" jsonschema:"spoken service name from the catalog"`
	Since string `json:"since,omitempty" jsonschema:"how far back to read, for example 15m or 1h (default 15m)"`
	Grep  string `json:"grep,omitempty" jsonschema:"case-insensitive substring filter"`
	Limit int    `json:"limit,omitempty" jsonschema:"maximum number of log lines (default 50)"`
}

type logsDetail struct {
	Service   string `json:"service"`
	Source    string `json:"source"`
	Since     string `json:"since"`
	Lines     int    `json:"lines"`
	Grep      string `json:"grep,omitempty"`
	Text      string `json:"text"`
	Truncated bool   `json:"truncated"`
}

func (d *Deps) infraLogs(ctx context.Context, _ *mcp.CallToolRequest, in logsInput) (*mcp.CallToolResult, any, error) {
	service, err := d.resolve(in.Name)
	if err != nil {
		return Failure("Не нашёл сервис "+strings.TrimSpace(in.Name)+".", map[string]string{"error": err.Error()})
	}
	since := 15 * time.Minute
	if strings.TrimSpace(in.Since) != "" {
		parsed, err := time.ParseDuration(strings.TrimSpace(in.Since))
		if err != nil || parsed <= 0 || parsed > 24*time.Hour {
			return Failure("Не понял период, используйте например 15m или 1h.", map[string]string{"since": in.Since})
		}
		since = parsed
	}
	limit := in.Limit
	if limit <= 0 {
		limit = 50
	}
	if limit > 500 {
		limit = 500
	}
	grep := strings.ToLower(strings.TrimSpace(in.Grep))
	display := DisplayName(service)
	detail := logsDetail{Service: service.ID, Since: since.String(), Grep: in.Grep}

	var text string
	switch service.Logs.Source {
	case catalog.LogsLoki:
		if d.Loki == nil {
			return Failure("Loki не настроен, логи недоступны.", detail)
		}
		detail.Source = catalog.LogsLoki
		callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
		lines, err := d.Loki.Query(callCtx, service.Logs.Selector, since, limit)
		cancel()
		if err != nil {
			return Failure("Не удалось прочитать логи "+display+" из Loki.", map[string]string{"error": err.Error()})
		}
		var b strings.Builder
		for _, line := range lines {
			if grep != "" && !strings.Contains(strings.ToLower(line.Text), grep) {
				continue
			}
			b.WriteString(line.Text)
			b.WriteString("\n")
			detail.Lines++
		}
		text = b.String()
	case catalog.LogsPods:
		if d.Kube == nil {
			return Failure("Kubernetes API не настроен, логи недоступны.", detail)
		}
		detail.Source = catalog.LogsPods
		text, err = d.podLogs(ctx, service, since, limit, grep, &detail)
		if err != nil {
			return Failure("Не удалось прочитать логи "+display+" из подов.", map[string]string{"error": err.Error()})
		}
	default:
		detail.Source = catalog.LogsNone
		return respond(Result{Speech: "Логи для " + display + " не настроены.", Details: detail})
	}

	redacted := redact.Text(text)
	if len(redacted) > d.logByteCap() {
		redacted = redacted[:d.logByteCap()]
		detail.Truncated = true
	}
	detail.Text = redacted
	if detail.Lines == 0 {
		detail.Lines = strings.Count(strings.TrimSpace(redacted), "\n") + boolToInt(strings.TrimSpace(redacted) != "")
	}
	return respond(Result{
		Speech:  fmt.Sprintf("Логи %s за %s: %d строк.", display, humanDuration(since), detail.Lines),
		Details: detail,
	})
}

func (d *Deps) podLogs(ctx context.Context, service catalog.Service, since time.Duration, limit int, grep string, detail *logsDetail) (string, error) {
	callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
	defer cancel()
	selector := map[string]string{"app.kubernetes.io/name": service.Workload.Name}
	containers := append([]string(nil), service.Logs.Containers...)
	if w, err := d.Kube.GetWorkload(callCtx, service.Namespace, service.Workload.Kind, service.Workload.Name); err == nil {
		if len(w.Selector) > 0 {
			selector = w.Selector
		}
		if len(containers) == 0 {
			containers = w.Containers
		}
	}
	if len(containers) == 0 {
		containers = append(containers, service.Containers...)
	}
	if len(containers) == 0 {
		containers = []string{""}
	}
	pods, err := d.Kube.ListPods(callCtx, service.Namespace, selector)
	if err != nil {
		return "", err
	}
	const maxPods = 3
	if len(pods) > maxPods {
		pods = pods[:maxPods]
	}
	var b strings.Builder
	lines := 0
	for _, pod := range pods {
		for _, container := range containers {
			if lines >= limit {
				break
			}
			logCtx, logCancel := context.WithTimeout(ctx, d.queryTimeout())
			body, err := d.Kube.PodLogs(logCtx, service.Namespace, pod.Name, container, limit, since)
			logCancel()
			if err != nil {
				continue
			}
			header := "== " + pod.Name
			if container != "" {
				header += "/" + container
			}
			header += " =="
			fmt.Fprintf(&b, "%s\n", header)
			for _, line := range strings.Split(strings.TrimRight(body, "\n"), "\n") {
				if line == "" {
					continue
				}
				if grep != "" && !strings.Contains(strings.ToLower(line), grep) {
					continue
				}
				b.WriteString(line)
				b.WriteString("\n")
				lines++
				if lines >= limit {
					break
				}
			}
		}
	}
	if len(pods) == 0 {
		return "", fmt.Errorf("no pods found for %s", service.Workload.Name)
	}
	detail.Lines = lines
	return b.String(), nil
}

// --- infra_query ----------------------------------------------------------

type queryInput struct {
	PromQL string `json:"promql" jsonschema:"read-only instant PromQL expression"`
}

type queryDetail struct {
	PromQL  string          `json:"promql"`
	Samples []labeledSample `json:"samples"`
	Error   string          `json:"error,omitempty"`
}

type labeledSample struct {
	Labels map[string]string `json:"labels,omitempty"`
	Value  float64           `json:"value"`
	Raw    string            `json:"raw,omitempty"`
}

func (d *Deps) infraQuery(ctx context.Context, _ *mcp.CallToolRequest, in queryInput) (*mcp.CallToolResult, any, error) {
	promql := strings.TrimSpace(in.PromQL)
	if promql == "" {
		return Failure("Пустой запрос PromQL.", nil)
	}
	if len(promql) > 2000 {
		return Failure("Запрос слишком длинный.", map[string]int{"length": len(promql)})
	}
	if d.Prom == nil {
		return Failure("Prometheus не настроен.", nil)
	}
	callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
	defer cancel()
	samples, err := d.Prom.Query(callCtx, promql, 20)
	if err != nil {
		return Failure("Запрос не выполнен.", map[string]string{"error": err.Error(), "promql": promql})
	}
	detail := queryDetail{PromQL: promql, Samples: make([]labeledSample, 0, len(samples))}
	for _, sample := range samples {
		detail.Samples = append(detail.Samples, labeledSample{Labels: sample.Labels, Value: sample.Value, Raw: sample.Raw})
	}
	speech := fmt.Sprintf("Запрос вернул %d рядов.", len(samples))
	if len(samples) == 0 {
		speech = "Запрос не вернул данных."
	}
	return respond(Result{Speech: speech, Details: detail})
}

// --- infra_overview -------------------------------------------------------

func (d *Deps) infraOverview(_ context.Context, _ *mcp.CallToolRequest, _ struct{}) (*mcp.CallToolResult, any, error) {
	if d.Catalog == nil {
		return Failure("Каталог сервисов не загружен.", nil)
	}
	return respond(Result{
		Speech: fmt.Sprintf("Каталог: %d сервисов. Топология в деталях.", len(d.Catalog.Services)),
		Details: map[string]string{
			"overview": d.Catalog.OverviewText(),
			"catalog":  d.Catalog.CatalogText(),
		},
	})
}

func (d *Deps) readCatalogResource(_ context.Context, req *mcp.ReadResourceRequest) (*mcp.ReadResourceResult, error) {
	if d.Catalog == nil {
		return nil, fmt.Errorf("catalog is not loaded")
	}
	text := d.Catalog.OverviewText() + "\n\n" + d.Catalog.CatalogText()
	return &mcp.ReadResourceResult{
		Contents: []*mcp.ResourceContents{{
			URI:      req.Params.URI,
			MIMEType: "text/markdown",
			Text:     text,
		}},
	}, nil
}

func (d *Deps) resolve(name string) (catalog.Service, error) {
	if d.Catalog == nil {
		return catalog.Service{}, fmt.Errorf("catalog is not loaded")
	}
	return d.Catalog.Resolve(name)
}

func humanDuration(d time.Duration) string {
	switch {
	case d >= time.Hour && d%time.Hour == 0:
		return fmt.Sprintf("%d ч", int(d.Hours()))
	case d >= time.Minute:
		return fmt.Sprintf("%d мин", int(d.Minutes()))
	default:
		return fmt.Sprintf("%d с", int(d.Seconds()))
	}
}

func boolToInt(v bool) int {
	if v {
		return 1
	}
	return 0
}
