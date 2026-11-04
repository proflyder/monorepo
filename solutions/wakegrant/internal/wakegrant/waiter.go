package wakegrant

import "sync"

// waiterHub wakes long-poll claims when a matching grant is created.
//
// Notifications are per (machine, purpose): an unlock waiter must not wake for
// a logon grant and vice versa. The number of concurrent waits is capped per
// machine across purposes, so one machine cannot pin an unbounded number of
// HTTP requests.
type waiterHub struct {
	mu     sync.Mutex
	counts map[string]int
	queues map[string]*waiterQueue
}

type waiterQueue struct {
	ch chan struct{}
}

func newWaiterHub() *waiterHub {
	return &waiterHub{
		counts: map[string]int{},
		queues: map[string]*waiterQueue{},
	}
}

func waiterKey(machine string, purpose Purpose) string {
	return machine + "\x00" + string(purpose)
}

// acquire registers a waiter. It returns false when the machine already has
// MaxWaitersPerQueue active waits. release must always be called.
func (h *waiterHub) acquire(machine string, purpose Purpose) (<-chan struct{}, func(), bool) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.counts[machine] >= MaxWaitersPerQueue {
		return nil, nil, false
	}
	h.counts[machine]++
	key := waiterKey(machine, purpose)
	queue, ok := h.queues[key]
	if !ok {
		queue = &waiterQueue{ch: make(chan struct{})}
		h.queues[key] = queue
	}
	ch := queue.ch
	release := func() {
		h.mu.Lock()
		defer h.mu.Unlock()
		if n := h.counts[machine] - 1; n > 0 {
			h.counts[machine] = n
		} else {
			delete(h.counts, machine)
		}
	}
	return ch, release, true
}

// broadcast wakes every waiter for (machine, purpose). It runs after the grant
// is committed, so a waiter that re-checks the store is guaranteed to see it.
func (h *waiterHub) broadcast(machine string, purpose Purpose) {
	h.mu.Lock()
	defer h.mu.Unlock()
	queue, ok := h.queues[waiterKey(machine, purpose)]
	if !ok {
		return
	}
	close(queue.ch)
	queue.ch = make(chan struct{})
}
