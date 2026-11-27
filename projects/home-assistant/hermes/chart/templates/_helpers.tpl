{{- define "hermes.name" -}}
{{- printf "hermes-%s" (required "instance is required" .Values.instance) -}}
{{- end -}}

{{- define "hermes.labels" -}}
app.kubernetes.io/name: hermes
app.kubernetes.io/instance: {{ include "hermes.name" . }}
proflyder.dev/hermes-instance: {{ .Values.instance | quote }}
{{- end -}}

{{- define "hermes.selector" -}}
app.kubernetes.io/name: hermes
app.kubernetes.io/instance: {{ include "hermes.name" . }}
{{- end -}}

{{- define "hermes.ownerOnly" -}}
{{- $telegram := dig "platforms" "telegram" dict .Values.config -}}
{{- if not (hasKey .Values.config "unauthorized_dm_behavior") -}}
{{- fail "config.unauthorized_dm_behavior must be ignore" -}}
{{- end -}}
{{- if ne (toString .Values.config.unauthorized_dm_behavior) "ignore" -}}
{{- fail "config.unauthorized_dm_behavior must be ignore" -}}
{{- end -}}
{{- if ne (toString (dig "extra" "unauthorized_dm_behavior" "ignore" $telegram)) "ignore" -}}
{{- fail "platforms.telegram.extra.unauthorized_dm_behavior must be ignore" -}}
{{- end -}}
{{- range $name := list "GATEWAY_ALLOW_ALL_USERS" "TELEGRAM_GROUP_ALLOWED_USERS" "TELEGRAM_GROUP_ALLOWED_CHATS" "TELEGRAM_GUEST_MODE" "GATEWAY_ALLOWED_USERS" -}}
{{- if hasKey $.Values.env $name -}}
{{- fail (printf "env.%s is forbidden: the instance answers only its owner" $name) -}}
{{- end -}}
{{- range $.Values.secretEnv -}}
{{- if eq .name $name -}}
{{- fail (printf "secretEnv %s is forbidden: the instance answers only its owner" $name) -}}
{{- end -}}
{{- end -}}
{{- end -}}
{{- $owner := false -}}
{{- range .Values.secretEnv -}}
{{- if eq .name "TELEGRAM_ALLOWED_USERS" -}}
{{- $owner = true -}}
{{- end -}}
{{- end -}}
{{- if not $owner -}}
{{- fail "secretEnv must provide TELEGRAM_ALLOWED_USERS with the owner id" -}}
{{- end -}}
{{- end -}}
