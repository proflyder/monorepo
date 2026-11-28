path "secret/data/*" {
  capabilities = ["read", "update", "delete"]
}

path "secret/metadata/*" {
  capabilities = ["read", "list"]
}

path "secret/metadata" {
  capabilities = ["list"]
}

path "secret/destroy/*" {
  capabilities = ["deny"]
}

path "secret/delete/*" {
  capabilities = ["deny"]
}

path "secret/undelete/*" {
  capabilities = ["deny"]
}

path "secret/subkeys/*" {
  capabilities = ["deny"]
}

path "secret/config" {
  capabilities = ["deny"]
}

path "sys/*" {
  capabilities = ["deny"]
}

path "auth/*" {
  capabilities = ["deny"]
}

path "identity/*" {
  capabilities = ["deny"]
}

path "secret/data/openbao" {
  capabilities = ["deny"]
}

path "secret/data/openbao/*" {
  capabilities = ["deny"]
}

path "secret/metadata/openbao" {
  capabilities = ["deny"]
}

path "secret/metadata/openbao/*" {
  capabilities = ["deny"]
}

path "secret/data/infra/vpn/client-ca" {
  capabilities = ["deny"]
}

path "secret/data/infra/vpn/client-ca/*" {
  capabilities = ["deny"]
}

path "secret/metadata/infra/vpn/client-ca" {
  capabilities = ["deny"]
}

path "secret/metadata/infra/vpn/client-ca/*" {
  capabilities = ["deny"]
}

path "secret/data/k8s/flux-system" {
  capabilities = ["deny"]
}

path "secret/data/k8s/flux-system/*" {
  capabilities = ["deny"]
}

path "secret/metadata/k8s/flux-system" {
  capabilities = ["deny"]
}

path "secret/metadata/k8s/flux-system/*" {
  capabilities = ["deny"]
}

path "secret/data/k8s/home-assistant/scheduler-auth" {
  capabilities = ["deny"]
}

path "secret/data/k8s/home-assistant/scheduler-auth/*" {
  capabilities = ["deny"]
}

path "secret/metadata/k8s/home-assistant/scheduler-auth" {
  capabilities = ["deny"]
}

path "secret/metadata/k8s/home-assistant/scheduler-auth/*" {
  capabilities = ["deny"]
}

path "secret/data/k8s/home-assistant/secrets-gateway" {
  capabilities = ["deny"]
}

path "secret/data/k8s/home-assistant/secrets-gateway/*" {
  capabilities = ["deny"]
}

path "secret/metadata/k8s/home-assistant/secrets-gateway" {
  capabilities = ["deny"]
}

path "secret/metadata/k8s/home-assistant/secrets-gateway/*" {
  capabilities = ["deny"]
}

path "secret/data/k8s/hermes-work" {
  capabilities = ["deny"]
}

path "secret/data/k8s/hermes-work/*" {
  capabilities = ["deny"]
}

path "secret/metadata/k8s/hermes-work" {
  capabilities = ["deny"]
}

path "secret/metadata/k8s/hermes-work/*" {
  capabilities = ["deny"]
}
