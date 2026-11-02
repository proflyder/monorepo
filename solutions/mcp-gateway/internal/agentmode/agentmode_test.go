package agentmode

import "testing"

func TestStateTransitions(t *testing.T) {
	state := New()
	if _, ok := state.Active(); ok {
		t.Fatal("new state is active")
	}
	state.Enter("thr_1")
	if got, ok := state.Active(); !ok || got != "thr_1" {
		t.Fatalf("Active = %q, %v", got, ok)
	}
	state.Exit()
	if _, ok := state.Active(); ok {
		t.Fatal("Exit kept the thread")
	}
}

func TestClearIfIgnoresAnotherThread(t *testing.T) {
	state := New()
	state.Enter("thr_new")
	state.ClearIf("thr_old")
	if got, ok := state.Active(); !ok || got != "thr_new" {
		t.Fatalf("Active = %q, %v", got, ok)
	}
	state.ClearIf("thr_new")
	if _, ok := state.Active(); ok {
		t.Fatal("ClearIf did not clear the matching thread")
	}
}
