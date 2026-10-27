{{- define "runtime.haProxyConfig" -}}
global
  maxconn 256
defaults
  mode tcp
  timeout connect 5s
  timeout client 1h
  timeout server 1h
frontend home_assistant
  bind 0.0.0.0:8123
  default_backend haos
backend haos
  server haos {{ required "haProxy.upstream must be HAOS host:8123" .Values.haProxy.upstream }} check
{{- if .Values.haProxy.privateBridge.enabled }}
frontend voice_proxy_ingress
  bind {{ required "privateBridge.bindAddress required" .Values.haProxy.privateBridge.bindAddress }}:8643
  tcp-request connection reject if !{ src {{ required "privateBridge.allowedClient required" .Values.haProxy.privateBridge.allowedClient }} }
  default_backend voice_proxy
backend voice_proxy
  server local_voice 127.0.0.1:8643 check
{{- if .Values.haProxy.privateBridge.matterEnabled }}
frontend matter_api_ingress
  bind {{ .Values.haProxy.privateBridge.bindAddress }}:5580
  tcp-request connection reject if !{ src {{ .Values.haProxy.privateBridge.allowedClient }} }
  default_backend matter_api
backend matter_api
  server local_matter 127.0.0.1:5580 check
{{- end }}
{{- end }}
{{- end -}}
