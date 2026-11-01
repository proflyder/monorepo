package tools

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/catalog"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/kubernetes"
	"ricardo.local/monorepo/solutions/mcp-gateway/internal/pending"
)

// ActionRolloutRestart is the only action kind the gateway can execute.
const ActionRolloutRestart = "rollout-restart"

// FixGroup is the infra.fix tool group.
type FixGroup struct{ Deps *Deps }

// Name implements group.Group.
func (g *FixGroup) Name() string { return GroupFix }

// Register implements group.Group.
func (g *FixGroup) Register(r *mcp.Server) {
	d := g.Deps
	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_prepare_restart",
		Description: "Validate that one catalog service may be restarted now and store exactly one pending action for 90 seconds. Returns a short Russian confirmation question; nothing is restarted yet.",
		Annotations: destructive(),
	}, instrument(d, "infra_prepare_restart", d.infraPrepareRestart))

	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_confirm",
		Description: "Execute the pending restart prepared by infra_prepare_restart. Takes no arguments, so it works even if the client lost conversation history. Self-disrupting services restart after a short delay.",
		Annotations: destructive(),
	}, instrument(d, "infra_confirm", d.infraConfirm))

	mcp.AddTool(r, &mcp.Tool{
		Name:        "infra_cancel",
		Description: "Drop the pending restart without executing it.",
		Annotations: destructive(),
	}, instrument(d, "infra_cancel", d.infraCancel))
}

type restartInput struct {
	Name string `json:"name" jsonschema:"spoken service name from the catalog"`
}

type prepareDetail struct {
	Allowed   bool   `json:"allowed"`
	Service   string `json:"service"`
	Namespace string `json:"namespace,omitempty"`
	Kind      string `json:"kind,omitempty"`
	Name      string `json:"name,omitempty"`
	Warning   string `json:"warning,omitempty"`
	ExpiresAt string `json:"expiresAt,omitempty"`
	Reason    string `json:"reason,omitempty"`
}

func (d *Deps) infraPrepareRestart(ctx context.Context, _ *mcp.CallToolRequest, in restartInput) (*mcp.CallToolResult, any, error) {
	service, err := d.resolve(in.Name)
	if err != nil {
		return Failure("Не нашёл сервис "+strings.TrimSpace(in.Name)+".", prepareDetail{Reason: err.Error()})
	}
	display := DisplayName(service)
	detail := prepareDetail{Service: service.ID}

	if service.Kind != catalog.ServiceK8s {
		detail.Reason = "external service"
		return respond(Result{Speech: "Перезапуск " + display + " через шлюз недоступен.", Details: detail})
	}
	detail.Namespace = service.Namespace
	detail.Kind = service.Workload.Kind
	detail.Name = service.Workload.Name
	if !service.Restart.Allowed {
		detail.Reason = "restart is not allowed by the catalog"
		return respond(Result{Speech: "Перезапуск " + display + " запрещён.", Details: detail})
	}
	detail.Warning = service.Restart.Warning
	if d.Kube == nil {
		detail.Reason = "kubernetes is not configured"
		return Failure("Кластер недоступен, перезапуск невозможен.", detail)
	}
	if d.Pending == nil {
		return Failure("Хранилище отложенных действий недоступно.", detail)
	}

	callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
	defer cancel()
	workload, err := d.Kube.GetWorkload(callCtx, service.Namespace, service.Workload.Kind, service.Workload.Name)
	if err != nil {
		detail.Reason = err.Error()
		return Failure("Не нашёл "+display+" в кластере.", detail)
	}
	if workload.RolloutInProgress() {
		detail.Reason = "rollout in progress"
		return respond(Result{Speech: display + " уже обновляется, подождите.", Details: detail})
	}
	if last, ok := kubernetes.LastRestart(workload); ok {
		elapsed := d.now().Sub(last)
		if elapsed < d.cooldown() {
			remaining := d.cooldown() - elapsed
			detail.Reason = "cooldown"
			return respond(Result{
				Speech:  fmt.Sprintf("%s перезапускали %s назад, подождите ещё %s.", display, humanDuration(elapsed), humanDuration(remaining)),
				Details: detail,
			})
		}
	}

	action := pending.Action{
		ServiceID:      service.ID,
		Namespace:      service.Namespace,
		WorkloadKind:   service.Workload.Kind,
		WorkloadName:   service.Workload.Name,
		Kind:           ActionRolloutRestart,
		SelfDisrupting: service.Restart.SelfDisrupting,
		Delay:          d.selfRestartDelay(),
		Warning:        service.Restart.Warning,
		CreatedAt:      d.now(),
	}
	d.Pending.Put(action)
	detail.Allowed = true
	detail.ExpiresAt = action.CreatedAt.Add(d.Pending.TTL()).UTC().Format(time.RFC3339)

	question := fmt.Sprintf("Перезапустить %s?", strings.ToLower(display))
	if warning := strings.TrimSpace(service.Restart.Warning); warning != "" {
		question += " " + warning
	}
	d.logger().Info("restart prepared", "event", "action_prepared", "action", ActionRolloutRestart,
		"target", service.ID, "namespace", service.Namespace, "workload", service.Workload.Name)
	return respond(Result{Speech: question, Details: detail})
}

type confirmDetail struct {
	Executed  bool   `json:"executed"`
	Scheduled bool   `json:"scheduled"`
	Service   string `json:"service,omitempty"`
	Kind      string `json:"kind,omitempty"`
	Name      string `json:"name,omitempty"`
	Namespace string `json:"namespace,omitempty"`
	Delay     string `json:"delay,omitempty"`
	Reason    string `json:"reason,omitempty"`
	Error     string `json:"error,omitempty"`
}

func (d *Deps) infraConfirm(ctx context.Context, _ *mcp.CallToolRequest, _ struct{}) (*mcp.CallToolResult, any, error) {
	if d.Pending == nil {
		return Failure("Хранилище отложенных действий недоступно.", nil)
	}
	action, ok := d.Pending.Take()
	if !ok {
		return respond(Result{Speech: "Нет отложенного действия.", Details: confirmDetail{Reason: "no pending action"}})
	}
	service := catalog.Service{ID: action.ServiceID}
	if d.Catalog != nil {
		if found, ok := d.Catalog.Find(action.ServiceID); ok {
			service = found
		}
	}
	display := DisplayName(service)
	detail := confirmDetail{Service: action.ServiceID, Kind: action.WorkloadKind, Name: action.WorkloadName, Namespace: action.Namespace}

	if d.Kube == nil {
		d.audit("restart", action.ServiceID, "failure", "error", "kubernetes is not configured")
		return Failure("Кластер недоступен, перезапуск не выполнен.", detail)
	}

	if action.SelfDisrupting {
		delay := action.Delay
		if delay <= 0 {
			delay = d.selfRestartDelay()
		}
		if d.Timers == nil {
			d.audit("restart", action.ServiceID, "failure", "error", "no timer scheduler")
			return Failure("Планировщик отложенных действий недоступен.", detail)
		}
		d.Timers.AfterFunc(delay, func() {
			patchCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			defer cancel()
			if err := d.Kube.PatchRestart(patchCtx, action.Namespace, action.WorkloadKind, action.WorkloadName, d.now()); err != nil {
				d.audit("restart", action.ServiceID, "failure", "error", err.Error(), "delayed", true)
				return
			}
			d.audit("restart", action.ServiceID, "success", "delayed", true)
		})
		d.audit("restart", action.ServiceID, "scheduled", "delay_seconds", int(delay.Seconds()))
		detail.Scheduled = true
		detail.Delay = delay.String()
		return respond(Result{
			Speech:  fmt.Sprintf("Перезапущу %s через %d секунд.", strings.ToLower(display), int(delay.Seconds())),
			Details: detail,
		})
	}

	callCtx, cancel := context.WithTimeout(ctx, d.queryTimeout())
	defer cancel()
	if err := d.Kube.PatchRestart(callCtx, action.Namespace, action.WorkloadKind, action.WorkloadName, d.now()); err != nil {
		detail.Error = err.Error()
		d.audit("restart", action.ServiceID, "failure", "error", err.Error())
		return Failure("Не удалось перезапустить "+display+".", detail)
	}
	d.audit("restart", action.ServiceID, "success")
	detail.Executed = true
	return respond(Result{Speech: "Перезапускаю " + strings.ToLower(display) + ".", Details: detail})
}

type cancelDetail struct {
	Cancelled bool   `json:"cancelled"`
	Service   string `json:"service,omitempty"`
}

func (d *Deps) infraCancel(_ context.Context, _ *mcp.CallToolRequest, _ struct{}) (*mcp.CallToolResult, any, error) {
	if d.Pending == nil {
		return Failure("Хранилище отложенных действий недоступно.", nil)
	}
	action, ok := d.Pending.Peek()
	d.Pending.Clear()
	if !ok {
		return respond(Result{Speech: "Нечего отменять.", Details: cancelDetail{}})
	}
	d.audit("restart", action.ServiceID, "cancelled")
	return respond(Result{Speech: "Отменил перезапуск.", Details: cancelDetail{Cancelled: true, Service: action.ServiceID}})
}
