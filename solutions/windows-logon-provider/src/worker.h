// The single background poller.
//
// Reads the configuration, then loops "challenge -> claim" until a valid grant
// is armed, the 90 s budget since provider load is exhausted, or the session is
// cancelled. It never blocks a LogonUI thread and it never survives past its
// budget.

#pragma once

#include <memory>

#include "provider.h"

namespace ricardo {
namespace komp {

constexpr unsigned long long kWorkerBudgetMs = 90ull * 1000ull;
constexpr unsigned kPollIntervalMs = 2000;

// Creates the worker thread. Caller must have set session->running.
void StartWorkerThread(const std::shared_ptr<Session>& session);

}  // namespace komp
}  // namespace ricardo
