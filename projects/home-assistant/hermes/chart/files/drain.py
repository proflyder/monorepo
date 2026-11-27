import time

try:
    from gateway.drain_control import write_drain_request
    from gateway.status import parse_active_agents, read_runtime_status

    start = time.monotonic()
    write_drain_request(principal="kubelet-prestop", suppress_notification=True)
    time.sleep(2)
    while time.monotonic() - start < 60:
        record = read_runtime_status() or {}
        if parse_active_agents(record.get("active_agents")) == 0:
            break
        time.sleep(1)
except Exception:
    pass
