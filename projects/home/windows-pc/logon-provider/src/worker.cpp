#include "worker.h"

#include <windows.h>

#include <algorithm>
#include <memory>
#include <string>

#include "config.h"
#include "http.h"
#include "log.h"
#include "protocol/protocol.h"

namespace ricardo {
namespace komp {
namespace {

constexpr unsigned kBackoffIntervalMs = 4000;

struct StartArgs {
    std::shared_ptr<Session> session;
};

struct RoundOutcome {
    enum class Kind { Armed, Retry, Fatal } kind = Kind::Retry;
    unsigned delay_ms = kPollIntervalMs;
};

std::string Number(unsigned long long value) { return std::to_string(value); }

bool Cancelled(const Session& session, unsigned generation) {
    return session.cancel_generation.load() != generation;
}

void SleepInterruptible(const Session& session, unsigned generation, unsigned milliseconds) {
    const ULONGLONG deadline = GetTickCount64() + milliseconds;
    while (GetTickCount64() < deadline) {
        if (Cancelled(session, generation)) return;
        Sleep(100);
    }
}

RoundOutcome Retry(unsigned delay_ms) {
    RoundOutcome outcome;
    outcome.kind = RoundOutcome::Kind::Retry;
    outcome.delay_ms = delay_ms;
    return outcome;
}

RoundOutcome Fatal() {
    RoundOutcome outcome;
    outcome.kind = RoundOutcome::Kind::Fatal;
    return outcome;
}

// One challenge/claim exchange. Any unexpected answer ends the round without
// arming; only a verified grant arms the session.
RoundOutcome OneRound(const std::shared_ptr<Session>& session,
                      unsigned generation,
                      const Config& config,
                      const wakegrant::ServiceEndpoint& endpoint,
                      const wakegrant::MachineSecret& secret,
                      unsigned long long elapsed_ms) {
    const std::string prefix = "elapsed_ms=" + Number(elapsed_ms);

    HttpResult challenge =
        HttpPostJson(endpoint, wakegrant::endpoint_path(endpoint, "/v1/challenge"),
                     wakegrant::challenge_request_json(config.machine));
    if (!challenge.ok) {
        LogEvent("http", "request=challenge ok=0 error=" + challenge.error +
                             " ms=" + Number(challenge.elapsed_ms) + " " + prefix);
        return Retry(kPollIntervalMs);
    }
    if (challenge.status != 200) {
        LogEvent("http", "request=challenge ok=1 status=" + Number(static_cast<unsigned long long>(challenge.status)) +
                             " ms=" + Number(challenge.elapsed_ms) + " " + prefix);
        return (challenge.status == 429 || challenge.status >= 500) ? Retry(kBackoffIntervalMs) : Fatal();
    }

    wakegrant::ChallengeResponse response;
    std::string error;
    if (!wakegrant::parse_challenge_response(challenge.body, &response, &error)) {
        LogEvent("challenge", "ok=0 error=" + error + " " + prefix);
        return Fatal();
    }

    const std::string mac =
        wakegrant::hmac_sha256_b64(secret.key,
                                   wakegrant::claim_mac_message(config.machine, response.nonce));
    if (mac.empty()) {
        LogEvent("claim", "ok=0 error=hmac_failed " + prefix);
        return Fatal();
    }

    // machine, nonce and mac are restricted to [a-z0-9-] and base64 characters,
    // so they need no JSON escaping.
    const std::string body = "{\"machine\":\"" + config.machine + "\",\"nonce\":\"" +
                             response.nonce + "\",\"mac\":\"" + mac + "\"}";

    HttpResult claim = HttpPostJson(endpoint, wakegrant::endpoint_path(endpoint, "/v1/claim"), body);
    if (!claim.ok) {
        LogEvent("http", "request=claim ok=0 error=" + claim.error +
                             " ms=" + Number(claim.elapsed_ms) + " " + prefix);
        return Retry(kPollIntervalMs);
    }
    if (claim.status != 200) {
        LogEvent("http", "request=claim ok=1 status=" + Number(static_cast<unsigned long long>(claim.status)) +
                             " ms=" + Number(claim.elapsed_ms) + " " + prefix);
        return (claim.status == 429 || claim.status >= 500) ? Retry(kBackoffIntervalMs) : Fatal();
    }

    const wakegrant::ClaimResult result =
        wakegrant::verify_claim_response(claim.body, secret.key, config.machine, response.nonce);
    switch (result.outcome) {
        case wakegrant::ClaimOutcome::Granted:
            LogEvent("grant", "result=granted grant_id=" + result.grant_id + " ms=" +
                                  Number(claim.elapsed_ms) + " " + prefix);
            // The grant is already consumed, so never claim again: only wait a
            // moment for Advise() and then give up.
            for (int attempt = 0; attempt < 20; ++attempt) {
                const ArmResult arm = ArmSession(session, generation);
                if (arm == ArmResult::Armed) {
                    LogEvent("armed", "ok=1");
                    return RoundOutcome{RoundOutcome::Kind::Armed, 0};
                }
                if (arm == ArmResult::Cancelled || Cancelled(*session, generation)) {
                    LogEvent("armed", "ok=0 reason=cancelled");
                    return Fatal();
                }
                SleepInterruptible(*session, generation, 100);
            }
            LogEvent("armed", "ok=0 reason=no_events");
            return Fatal();
        case wakegrant::ClaimOutcome::Denied:
            LogEvent("grant", "result=denied ms=" + Number(claim.elapsed_ms) + " " + prefix);
            return Retry(kPollIntervalMs);
        case wakegrant::ClaimOutcome::InvalidMac:
        case wakegrant::ClaimOutcome::Malformed:
            LogEvent("grant", std::string("result=") +
                                  (result.outcome == wakegrant::ClaimOutcome::InvalidMac
                                       ? "invalid_mac"
                                       : "malformed") +
                                  " detail=" + result.detail + " ms=" + Number(claim.elapsed_ms) +
                                  " " + prefix);
            return Retry(kPollIntervalMs);
    }
    return Retry(kPollIntervalMs);
}

void RunWorker(const std::shared_ptr<Session>& session, unsigned generation) {
    LogEvent("worker", "phase=start budget_ms=" + Number(kWorkerBudgetMs));

    Config config;
    std::string error;
    const ConfigState state = LoadConfig(&config, &error);
    if (state == ConfigState::Disabled) {
        LogEvent("config", "state=disabled");
        return;
    }
    if (state != ConfigState::Ok) {
        LogEvent("config", "state=invalid error=" + error);
        return;
    }
    LogEvent("config", "state=ok machine=" + config.machine + " service=" +
                           wakegrant::redact_service_url(config.service_url));

    wakegrant::MachineSecret secret;
    if (!LoadSecret(&secret, &error)) {
        LogEvent("secret", "ok=0 error=" + error);
        return;
    }
    LogEvent("secret", "ok=1");

    wakegrant::ServiceEndpoint endpoint;
    if (!wakegrant::parse_service_url(config.service_url, &endpoint, &error)) {
        LogEvent("config", "state=invalid error=" + error);
        ClearSecret(&secret);
        return;
    }

    const auto elapsed = [&session]() {
        return static_cast<unsigned long long>(GetTickCount64() - session->load_tick);
    };

    bool armed = false;
    bool stopped = false;
    while (!armed && !stopped) {
        if (Cancelled(*session, generation)) break;
        const unsigned long long now = elapsed();
        if (now >= kWorkerBudgetMs) break;

        RoundOutcome outcome = OneRound(session, generation, config, endpoint, secret, now);
        switch (outcome.kind) {
            case RoundOutcome::Kind::Armed:
                armed = true;
                break;
            case RoundOutcome::Kind::Fatal:
                stopped = true;
                break;
            case RoundOutcome::Kind::Retry: {
                const unsigned long long now_ms = elapsed();
                const unsigned long long left = now_ms >= kWorkerBudgetMs ? 0 : kWorkerBudgetMs - now_ms;
                const unsigned delay =
                    static_cast<unsigned>(std::min<unsigned long long>(outcome.delay_ms, left));
                if (delay > 0) SleepInterruptible(*session, generation, delay);
                break;
            }
        }
    }

    ClearSecret(&secret);
    if (Cancelled(*session, generation)) {
        LogEvent("worker", "phase=cancelled");
    } else if (armed) {
        LogEvent("worker", "phase=armed");
    } else {
        LogEvent("worker", "phase=stopped");
    }
}

DWORD WINAPI WorkerThreadProc(void* parameter) {
    std::unique_ptr<StartArgs> args(static_cast<StartArgs*>(parameter));
    const std::shared_ptr<Session> session = args->session;
    const unsigned generation = session->cancel_generation.load();
    try {
        RunWorker(session, generation);
    } catch (...) {
        LogEvent("worker", "phase=exception");
    }
    session->running.store(false);
    g_active_workers.fetch_sub(1);
    return 0;
}

}  // namespace

void StartWorkerThread(const std::shared_ptr<Session>& session) {
    auto* args = new (std::nothrow) StartArgs{session};
    if (args == nullptr) {
        session->running.store(false);
        LogEvent("worker", "phase=start_failed error=nomem");
        return;
    }
    g_active_workers.fetch_add(1);
    HANDLE thread = CreateThread(nullptr, 0, WorkerThreadProc, args, 0, nullptr);
    if (thread == nullptr) {
        g_active_workers.fetch_sub(1);
        session->running.store(false);
        delete args;
        LogEvent("worker", "phase=start_failed");
        return;
    }
    CloseHandle(thread);
}

}  // namespace komp
}  // namespace ricardo
