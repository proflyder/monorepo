package tools

import (
	"context"
	"fmt"
	"net/url"
	"path"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/alertmanager"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/catalog"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/checks"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/kubernetes"
)

// reportTextCap keeps the Telegram-ready report below the message budget.
const reportTextCap = 3500

// Service health states used by infra_report.
const (
	statusOK       = "ok"
	statusDegraded = "degraded"
	statusDown     = "down"
	statusUnknown  = "unknown"
)

type serviceReport struct {
	ID        string          `json:"id"`
	Name      string          `json:"name"`
	Group     string          `json:"group,omitempty"`
	Kind      string          `json:"kind"`
	Namespace string          `json:"namespace,omitempty"`
	Status    string          `json:"status"`
	Ready     int64           `json:"ready,omitempty"`
	Desired   int64           `json:"desired,omitempty"`
	Restarts  int             `json:"restartsLastHour"`
	Image     string          `json:"image,omitempty"`
	Age       string          `json:"age,omitempty"`
	Checks    []checks.Result `json:"checks"`
	Versions  []string        `json:"versions,omitempty"`
	Alerts    []string        `json:"alerts,omitempty"`
	Issues    []string        `json:"issues,omitempty"`
}

type infraReportDetail struct {
	Verdict   string          `json:"verdict"`
	CheckedAt string          `json:"checkedAt"`
	Total     int             `json:"total"`
	OK        int             `json:"ok"`
	Problems  int             `json:"problems"`
	Unknown   int             `json:"unknown"`
	Alerts    []alertDetail   `json:"alerts"`
	Services  []serviceReport `json:"services"`
	Text      string          `json:"text"`
	Errors    []string        `json:"errors,omitempty"`
}

type infraReportInput struct {
	Verbose bool `json:"verbose,omitempty" jsonschema:"append a compact versions section (image, age, versions) to the text report"`
}

func (d *Deps) infraReport(ctx context.Context, _ *mcp.CallToolRequest, in infraReportInput) (*mcp.CallToolResult, any, error) {
	cat := d.Catalog
	if cat == nil {
		return Failure("Каталог сервисов не загружен.", nil)
	}
	ctx, cancel := context.WithTimeout(ctx, d.reportTimeout())
	defer cancel()

	var alerts []alertmanager.Alert
	var mu sync.Mutex
	detail := infraReportDetail{
		CheckedAt: d.now().UTC().Format(time.RFC3339),
		Total:     len(cat.Services),
		Alerts:    []alertDetail{},
		Services:  make([]serviceReport, len(cat.Services)),
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
			active, err := d.Alerts.Active(callCtx, 100)
			if err != nil {
				addError("алерты: " + err.Error())
				return
			}
			mu.Lock()
			alerts = active
			mu.Unlock()
		}()
	} else {
		addError("алерты: Alertmanager не настроен")
	}

	for index, service := range cat.Services {
		index, service := index, service
		wg.Add(1)
		go func() {
			defer wg.Done()
			detail.Services[index] = d.collectServiceReport(ctx, service)
		}()
	}

	wg.Wait()

	for _, alert := range alerts {
		if !cat.IgnoresAlert(alert.Labels["alertname"]) {
			detail.Alerts = append(detail.Alerts, compactAlert(alert))
		}
	}
	detail.Alerts = dedupeAlerts(detail.Alerts)

	// Related alerts require the full active set; attach them after the
	// parallel collection so a slow Alertmanager never blocks a service probe.
	for i := range detail.Services {
		for _, alert := range alerts {
			if alertMatches(alert, serviceByID(cat, detail.Services[i].ID)) {
				detail.Services[i].Alerts = append(detail.Services[i].Alerts, alert.Summary())
				detail.Services[i].Issues = append(detail.Services[i].Issues, "алерт: "+alert.Summary())
				if detail.Services[i].Status == statusOK {
					detail.Services[i].Status = statusDegraded
				}
			}
		}
	}

	for i := range detail.Services {
		switch detail.Services[i].Status {
		case statusOK:
			detail.OK++
		case statusUnknown:
			// An unreadable Kubernetes workload is a real gap in the check; an
			// external service with HA checks not configured is not.
			if detail.Services[i].Kind == catalog.ServiceK8s {
				detail.Problems++
			} else {
				detail.Unknown++
			}
		default:
			detail.Problems++
		}
	}
	detail.Verdict = reportVerdict(detail)
	detail.Text = formatServiceReport(detail, cat, in.Verbose)
	return respond(Result{Speech: detail.Verdict, Details: detail})
}

func serviceByID(cat *catalog.Catalog, id string) catalog.Service {
	service, _ := cat.Find(id)
	return service
}

func (d *Deps) collectServiceReport(ctx context.Context, service catalog.Service) serviceReport {
	item := serviceReport{
		ID:        service.ID,
		Name:      DisplayName(service),
		Group:     serviceGroup(service),
		Kind:      service.Kind,
		Namespace: service.Namespace,
		Status:    statusOK,
		Checks:    []checks.Result{},
	}
	if service.Kind == catalog.ServiceK8s {
		item = d.collectWorkloadReport(ctx, service, item)
	}
	if d.Checks != nil && len(service.Checks) > 0 {
		item.Checks = d.Checks.RunAll(ctx, []catalog.Service{service})
	}
	classifyService(&item, service)
	for _, check := range item.Checks {
		if check.Type == catalog.CheckHATemplate && check.OK && check.Value != "" && check.Value != "нет данных" {
			item.Versions = append(item.Versions, check.Label+": "+check.Value)
		}
	}
	if item.Status == statusUnknown && len(item.Checks) > 0 {
		allSkipped := true
		for _, check := range item.Checks {
			if !check.Skipped {
				allSkipped = false
				break
			}
		}
		if allSkipped {
			item.Issues = append(item.Issues, "проверки не настроены")
		}
	}
	return item

}

func (d *Deps) collectWorkloadReport(ctx context.Context, service catalog.Service, item serviceReport) serviceReport {
	if d.Kube == nil {
		item.Status = statusUnknown
		item.Issues = append(item.Issues, "Kubernetes API не настроен")
		return item
	}
	callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
	defer cancel()
	workload, err := d.Kube.GetWorkload(callCtx, service.Namespace, service.Workload.Kind, service.Workload.Name)
	if err != nil {
		item.Status = statusUnknown
		item.Issues = append(item.Issues, err.Error())
		return item
	}
	item.Ready, item.Desired = workload.Ready, workload.Desired
	pods, err := d.Kube.ListPods(callCtx, service.Namespace, workload.Selector)
	if err != nil {
		item.Issues = append(item.Issues, "поды: "+err.Error())
	} else {
		cutoff := d.now().Add(-time.Hour)
		oldest := time.Time{}
		for _, pod := range pods {
			if item.Image == "" {
				item.Image = shortImage(pickImage(pod, service))
			}
			if !pod.StartTime.IsZero() && (oldest.IsZero() || pod.StartTime.Before(oldest)) {
				oldest = pod.StartTime
			}
			for _, container := range pod.Containers {
				if !container.LastTerminatedAt.IsZero() && container.LastTerminatedAt.After(cutoff) {
					item.Restarts++
				}
			}
		}
		if !oldest.IsZero() {
			item.Age = humanAge(d.now().Sub(oldest))
		}
	}
	if !workload.AvailableOK() {
		if workload.Ready == 0 {
			item.Status = statusDown
		} else {
			item.Status = statusDegraded
		}
		item.Issues = append(item.Issues, fmt.Sprintf("готовы %d из %d", workload.Ready, workload.Desired))
	}
	return item
}

// classifyService merges check results into the workload-derived status.
func classifyService(item *serviceReport, service catalog.Service) {
	evaluated := 0
	failed := 0
	for _, check := range item.Checks {
		if check.Skipped {
			continue
		}
		evaluated++
		if check.OK {
			continue
		}
		failed++
		item.Issues = append(item.Issues, checkIssue(check))
	}
	if failed > 0 {
		if service.Kind == catalog.ServiceExternal && failed == evaluated {
			item.Status = statusDown
		} else if item.Status == statusOK {
			item.Status = statusDegraded
		}
	}
	if item.Restarts > 0 && item.Status == statusOK {
		item.Status = statusDegraded
		item.Issues = append(item.Issues, fmt.Sprintf("рестартов за час: %d", item.Restarts))
	}
	if service.Kind == catalog.ServiceExternal && evaluated == 0 {
		item.Status = statusUnknown
	}
}

func reportVerdict(detail infraReportDetail) string {
	if detail.Problems == 0 {
		verdict := fmt.Sprintf("Всё работает: %d из %d", detail.OK, detail.Total)
		if detail.Unknown > 0 {
			verdict += fmt.Sprintf(", не проверено %d", detail.Unknown)
		}
		return verdict + ". Прислать подробности в Telegram?"
	}
	var names []string
	for _, service := range detail.Services {
		if service.Status == statusOK {
			continue
		}
		names = append(names, service.Name)
	}
	shown := names
	if len(shown) > 2 {
		shown = shown[:2]
	}
	suffix := ""
	if len(names) > len(shown) {
		suffix = fmt.Sprintf(" и ещё %d", len(names)-len(shown))
	}
	return fmt.Sprintf("Проверено сервисов: %d, работают %d. Проблемы: %s%s. Прислать подробности в Telegram?",
		detail.Total, detail.OK, strings.Join(shown, ", "), suffix)
}

// reportHeadline is the second line of the Telegram text: a short verdict with
// an emoji. It never asks a question; the offered follow-up lives in speech.
func reportHeadline(detail infraReportDetail) string {
	if detail.Problems == 0 {
		line := fmt.Sprintf("✅ Всё работает: %d из %d", detail.OK, detail.Total)
		if detail.Unknown > 0 {
			line += fmt.Sprintf(", не проверено %d", detail.Unknown)
		}
		return line
	}
	return fmt.Sprintf("⚠️ Есть проблемы: %d из %d", detail.Problems, detail.Total)
}

func formatServiceReport(detail infraReportDetail, cat *catalog.Catalog, verbose bool) string {
	var b strings.Builder
	fmt.Fprintf(&b, "Отчёт по инфраструктуре, %s\n", humanStamp(detail.CheckedAt))
	b.WriteString(reportHeadline(detail) + "\n")
	if len(detail.Errors) > 0 {
		fmt.Fprintf(&b, "⚠️ Проверка неполная: %s\n", detail.Errors[0])
	}

	if detail.Problems > 0 {
		b.WriteString("\nПроблемы\n")
		for _, group := range reportGroups(detail, cat) {
			for _, service := range group.items {
				if !isProblem(service) {
					continue
				}
				writeProblem(&b, service)
			}
		}
	}

	if normal := normalGroupLines(detail, cat); len(normal) > 0 {
		b.WriteString("\nВ норме\n")
		for _, line := range normal {
			b.WriteString(line + "\n")
		}
	}

	if len(detail.Alerts) > 0 {
		b.WriteString("\nАлерты\n")
		for _, alert := range detail.Alerts {
			b.WriteString(alertLine(alert) + "\n")
		}
	}

	if verbose {
		if lines := versionLines(detail); len(lines) > 0 {
			b.WriteString("\nВерсии\n")
			for _, line := range lines {
				b.WriteString(line + "\n")
			}
		}
	}
	return capReport(strings.TrimSpace(b.String()))
}

func isProblem(service serviceReport) bool {
	if service.Status == statusOK {
		return false
	}
	if service.Status == statusUnknown && service.Kind == catalog.ServiceExternal {
		return false
	}
	return true
}

func writeProblem(b *strings.Builder, service serviceReport) {
	marker := "⚠️"
	if service.Status == statusDown {
		marker = "❌"
	}
	reason := problemReason(service)
	fmt.Fprintf(b, "%s %s — %s\n", marker, service.Name, reason)
	if facts := problemFacts(service, reason); len(facts) > 0 {
		b.WriteString("   " + strings.Join(facts, ", ") + "\n")
	}
}

func problemReason(service serviceReport) string {
	for _, issue := range service.Issues {
		if strings.TrimSpace(issue) != "" {
			return issue
		}
	}
	if len(service.Alerts) > 0 {
		return service.Alerts[0]
	}
	switch service.Status {
	case statusDown:
		return "недоступен"
	case statusUnknown:
		return "состояние неизвестно"
	default:
		return "работает неполностью"
	}
}

func problemFacts(service serviceReport, reason string) []string {
	var facts []string
	add := func(value string) {
		value = strings.TrimSpace(value)
		if value == "" || value == reason {
			return
		}
		for _, existing := range facts {
			if existing == value {
				return
			}
		}
		facts = append(facts, value)
	}
	if service.Kind == catalog.ServiceK8s {
		if service.Desired > 0 || service.Ready > 0 {
			add(fmt.Sprintf("готовы %d из %d", service.Ready, service.Desired))
		}
		add(fmt.Sprintf("рестартов 1ч: %d", service.Restarts))
	}
	for _, check := range service.Checks {
		if check.Skipped || check.OK {
			continue
		}
		add(checkIssue(check))
	}
	for _, alert := range service.Alerts {
		add("алерт: " + alert)
	}
	for _, issue := range service.Issues {
		add(issue)
	}
	if len(facts) > 5 {
		facts = facts[:5]
	}
	return facts
}

type reportGroup struct {
	name  string
	items []serviceReport
}

// normalGroupLines renders one short line per group of healthy services.
// Groups with more than five healthy services collapse to a count so the
// report stays scannable.
func normalGroupLines(detail infraReportDetail, cat *catalog.Catalog) []string {
	var lines []string
	for _, group := range reportGroups(detail, cat) {
		var names []string
		for _, service := range group.items {
			if service.Status == statusOK {
				names = append(names, service.Name)
			}
		}
		if len(names) == 0 {
			continue
		}
		if len(names) > 5 {
			lines = append(lines, fmt.Sprintf("✅ %s (%d)", group.name, len(names)))
			continue
		}
		lines = append(lines, "✅ "+group.name+": "+strings.Join(names, ", "))
	}
	return lines
}

// reportGroups keeps the catalog group order: first group_order, then any
// group that only appears in services in first-appearance order.
func reportGroups(detail infraReportDetail, cat *catalog.Catalog) []reportGroup {
	groups := make([]reportGroup, 0)
	index := make(map[string]int)
	add := func(name string, service serviceReport) {
		if name == "" {
			name = "Прочее"
		}
		if i, ok := index[name]; ok {
			groups[i].items = append(groups[i].items, service)
			return
		}
		index[name] = len(groups)
		groups = append(groups, reportGroup{name: name, items: []serviceReport{service}})
	}
	ordered := make(map[string]bool, len(cat.GroupOrder))
	for _, name := range cat.GroupOrder {
		ordered[name] = true
		for _, service := range detail.Services {
			if service.Group == name {
				add(name, service)
			}
		}
	}
	for _, service := range detail.Services {
		if ordered[service.Group] {
			continue
		}
		add(service.Group, service)
	}
	return groups
}

func alertLine(alert alertDetail) string {
	line := "⚠️ " + alert.Name
	if alert.Severity != "" {
		line += " (" + alert.Severity + ")"
	}
	if alert.Summary != "" {
		line += ": " + alert.Summary
	}
	return line
}

func versionLines(detail infraReportDetail) []string {
	var lines []string
	for _, service := range detail.Services {
		var facts []string
		if service.Image != "" {
			facts = append(facts, service.Image)
		}
		if service.Age != "" {
			facts = append(facts, "возраст "+service.Age)
		}
		facts = append(facts, service.Versions...)
		if len(facts) == 0 {
			continue
		}
		lines = append(lines, fmt.Sprintf("%s — %s", service.Name, strings.Join(facts, ", ")))
	}
	return lines
}

// serviceGroup returns the catalog group of a service. When the catalog does
// not set one, Kubernetes services fall back to their namespace and external
// services to their host title or address; a raw address is only a last
// resort for catalogs that predate the group field.
func serviceGroup(service catalog.Service) string {
	if group := strings.TrimSpace(service.Group); group != "" {
		return group
	}
	if service.Kind == catalog.ServiceK8s {
		return "K3s / " + service.Namespace
	}
	if host := strings.TrimSpace(service.HostTitle); host != "" {
		return host
	}
	for _, check := range service.Checks {
		switch check.Type {
		case catalog.CheckTCP:
			if check.Host != "" {
				return "LAN / " + check.Host
			}
		case catalog.CheckHTTP:
			if parsed, err := url.Parse(check.URL); err == nil && parsed.Hostname() != "" {
				return "LAN / " + parsed.Hostname()
			}
		}
	}
	return "Внешние"
}

func capReport(text string) string {
	if len([]rune(text)) <= reportTextCap {
		return text
	}
	runes := []rune(text)
	cut := string(runes[:reportTextCap])
	if idx := strings.LastIndexByte(cut, '\n'); idx > 0 {
		cut = cut[:idx]
	}
	return strings.TrimSpace(cut) + "\n… отчёт сокращён"
}

func dedupeAlerts(alerts []alertDetail) []alertDetail {
	seen := make(map[string]bool, len(alerts))
	out := make([]alertDetail, 0, len(alerts))
	for _, alert := range alerts {
		key := alert.Name + "|" + alert.Summary
		if seen[key] {
			continue
		}
		seen[key] = true
		out = append(out, alert)
	}
	sort.SliceStable(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out
}

// pickImage chooses the image that represents the workload, not a sidecar.
// Preference: the catalog containers[0], then the container named after the
// workload (or its suffix after the last dash), then pod spec order.
func pickImage(pod kubernetes.Pod, service catalog.Service) string {
	if len(service.Containers) > 0 {
		if image := imageOfContainer(pod, service.Containers[0]); image != "" {
			return image
		}
	}
	name := strings.TrimSpace(service.Workload.Name)
	if name != "" {
		candidates := []string{name}
		if idx := strings.LastIndex(name, "-"); idx >= 0 && idx < len(name)-1 {
			candidates = append(candidates, name[idx+1:])
		}
		for _, candidate := range candidates {
			if image := imageOfContainer(pod, candidate); image != "" {
				return image
			}
		}
	}
	for _, container := range pod.Containers {
		if container.Image != "" {
			return container.Image
		}
	}
	return ""
}

func imageOfContainer(pod kubernetes.Pod, name string) string {
	for _, container := range pod.Containers {
		if container.Name == name {
			return container.Image
		}
	}
	return ""
}

func shortImage(image string) string {
	image = strings.TrimSpace(image)
	if image == "" {
		return ""
	}
	if idx := strings.Index(image, "@sha256:"); idx >= 0 {
		digest := image[idx+len("@sha256:"):]
		if len(digest) > 8 {
			digest = digest[:8]
		}
		return path.Base(image[:idx]) + "@sha256:" + digest
	}
	return path.Base(image)
}

func humanAge(d time.Duration) string {
	switch {
	case d >= 24*time.Hour:
		return fmt.Sprintf("%dд", int(d.Hours()/24))
	case d >= time.Hour:
		return fmt.Sprintf("%dч", int(d.Hours()))
	case d >= time.Minute:
		return fmt.Sprintf("%dм", int(d.Minutes()))
	default:
		return fmt.Sprintf("%dс", int(d.Seconds()))
	}
}

func humanStamp(raw string) string {
	parsed, err := time.Parse(time.RFC3339, raw)
	if err != nil {
		return raw
	}
	return parsed.Format("02.01 15:04")
}
