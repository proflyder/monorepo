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

func (d *Deps) infraReport(ctx context.Context, _ *mcp.CallToolRequest, _ struct{}) (*mcp.CallToolResult, any, error) {
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
	detail.Text = formatServiceReport(detail, cat)
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
				for _, container := range pod.Containers {
					if container.Image != "" {
						item.Image = shortImage(container.Image)
						break
					}
				}
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
		item.Issues = append(item.Issues, check.Label+": "+check.Error)
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

func formatServiceReport(detail infraReportDetail, cat *catalog.Catalog) string {
	var b strings.Builder
	fmt.Fprintf(&b, "Отчёт по инфраструктуре, %s\n", humanStamp(detail.CheckedAt))
	fmt.Fprintf(&b, "%s\n", detail.Verdict)

	groups := make(map[string][]serviceReport)
	var order []string
	for _, service := range detail.Services {
		group := serviceGroup(serviceByID(cat, service.ID))
		if _, seen := groups[group]; !seen {
			order = append(order, group)
		}
		groups[group] = append(groups[group], service)
	}
	sort.Strings(order)
	for _, group := range order {
		fmt.Fprintf(&b, "\n%s\n", group)
		items := groups[group]
		sort.SliceStable(items, func(i, j int) bool { return items[i].ID < items[j].ID })
		for _, service := range items {
			b.WriteString("  " + serviceReportLine(service) + "\n")
		}
	}

	if len(detail.Alerts) > 0 {
		b.WriteString("\nАлерты\n")
		for _, alert := range detail.Alerts {
			line := "  - " + alert.Name
			if alert.Severity != "" {
				line += " (" + alert.Severity + ")"
			}
			if alert.Summary != "" {
				line += ": " + alert.Summary
			}
			b.WriteString(line + "\n")
		}
	}
	if len(detail.Errors) > 0 {
		b.WriteString("\nНе проверено\n")
		for _, err := range detail.Errors {
			b.WriteString("  - " + err + "\n")
		}
	}
	return capReport(strings.TrimSpace(b.String()))
}

func serviceReportLine(service serviceReport) string {
	var parts []string
	parts = append(parts, service.Name+": "+service.Status)
	if service.Kind == catalog.ServiceK8s {
		if service.Desired > 0 || service.Ready > 0 {
			parts = append(parts, fmt.Sprintf("%d/%d", service.Ready, service.Desired))
		}
		parts = append(parts, fmt.Sprintf("рестартов 1ч: %d", service.Restarts))
	}
	if service.Image != "" {
		parts = append(parts, service.Image)
	}
	if service.Age != "" {
		parts = append(parts, service.Age)
	}
	for _, version := range service.Versions {
		parts = append(parts, version)
	}
	if len(service.Issues) > 0 {
		issues := service.Issues
		if len(issues) > 3 {
			issues = issues[:3]
		}
		parts = append(parts, strings.Join(issues, "; "))
	} else {
		for _, check := range service.Checks {
			if !check.Skipped && check.OK {
				parts = append(parts, check.Label+" ok")
				break
			}
		}
	}
	return strings.Join(parts, ", ")
}

func serviceGroup(service catalog.Service) string {
	if service.Kind == catalog.ServiceK8s {
		return "K3s / " + service.Namespace
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
