#!/usr/bin/env bash
# Fresh Ubuntu guest only. Do not run against an existing cluster.
set -euo pipefail
[[ $(id -u) == 0 ]]
[[ ! -e /etc/rancher/k3s/config.yaml ]] || { echo 'Existing cluster configuration; refusing bootstrap' >&2; exit 1; }
: "${K3S_VERSION:?Source projects/home/kubernetes/versions.sh and export K3S_VERSION}"
iface=$(ip -j route get 1.1.1.1 | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["dev"])')
node_ip=$(ip -j route get 1.1.1.1 | python3 -c 'import json,sys;print(json.load(sys.stdin)[0]["prefsrc"])')
install -d -m 700 /etc/rancher/k3s /srv/k3s/recovery
cat > /etc/rancher/k3s/config.yaml <<CONFIG
node-name: lab-host
node-ip: $node_ip
flannel-iface: $iface
data-dir: /srv/k3s/runtime
write-kubeconfig-mode: "0600"
secrets-encryption: true
resolv-conf: /run/systemd/resolve/resolv.conf
disable:
  - traefik
  - servicelb
kubelet-arg:
  - container-log-max-size=10Mi
  - container-log-max-files=3
  - system-reserved=memory=512Mi,cpu=500m
  - kube-reserved=memory=512Mi,cpu=500m
CONFIG
chmod 600 /etc/rancher/k3s/config.yaml
curl -fsSL --retry 3 https://get.k3s.io -o /srv/k3s/recovery/install-k3s.sh
INSTALL_K3S_VERSION="$K3S_VERSION" sh /srv/k3s/recovery/install-k3s.sh
for attempt in $(seq 1 90); do
  k3s kubectl get node lab-host >/dev/null 2>&1 && break
  sleep 2
done
k3s kubectl wait node/lab-host --for=condition=Ready --timeout=180s
k3s kubectl create namespace home-assistant
k3s kubectl create namespace monitoring
cat <<'YAML' | k3s kubectl apply -f -
apiVersion: storage.k8s.io/v1
kind: StorageClass
metadata:
  name: local-retain
provisioner: rancher.io/local-path
reclaimPolicy: Retain
volumeBindingMode: WaitForFirstConsumer
YAML
