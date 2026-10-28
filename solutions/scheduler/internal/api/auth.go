package api

import (
	"context"
	"crypto/sha256"
	"crypto/subtle"
	"errors"
	"net/http"
	"strings"
	"time"

	"connectrpc.com/connect"
	"golang.org/x/net/http2"
	"golang.org/x/net/http2/h2c"

	ricardov1connect "ricardo.local/monorepo/packages/proto/ricardo/v1/ricardov1connect"
)

const (
	// MaxBodyBytes bounds every unary request and response body.
	MaxBodyBytes = 1 << 20
	bearerPrefix = "bearer "
)

// AuthInterceptor requires `Authorization: Bearer <token>` on every RPC.
// Comparison is constant-time over fixed-size digests; the token is never
// logged.
func AuthInterceptor(token string) connect.UnaryInterceptorFunc {
	expected := sha256.Sum256([]byte(token))
	return func(next connect.UnaryFunc) connect.UnaryFunc {
		return func(ctx context.Context, req connect.AnyRequest) (connect.AnyResponse, error) {
			header := req.Header().Get("Authorization")
			if !matchesBearer(header, expected) {
				return nil, connect.NewError(connect.CodeUnauthenticated, errors.New("unauthenticated"))
			}
			return next(ctx, req)
		}
	}
}

func matchesBearer(header string, expected [32]byte) bool {
	header = strings.TrimSpace(header)
	if len(header) <= len(bearerPrefix) || !strings.EqualFold(header[:len(bearerPrefix)], bearerPrefix) {
		return false
	}
	provided := strings.TrimSpace(header[len(bearerPrefix):])
	if provided == "" {
		return false
	}
	got := sha256.Sum256([]byte(provided))
	return subtle.ConstantTimeCompare(got[:], expected[:]) == 1
}

// NewHTTPHandler mounts both protobuf services on one listener. connect-go
// handlers speak Connect (JSON and binary), gRPC over HTTP/2 and gRPC-Web, so
// there is a single contract rather than a parallel REST facade.
func NewHTTPHandler(svc *Service, alarms *AlarmService, token string) http.Handler {
	opts := []connect.HandlerOption{
		connect.WithInterceptors(AuthInterceptor(token)),
		connect.WithReadMaxBytes(MaxBodyBytes),
		connect.WithSendMaxBytes(MaxBodyBytes),
	}
	mux := http.NewServeMux()
	schedulerPath, schedulerHandler := ricardov1connect.NewSchedulerServiceHandler(svc, opts...)
	mux.Handle(schedulerPath, schedulerHandler)
	alarmPath, alarmHandler := ricardov1connect.NewAlarmServiceHandler(alarms, opts...)
	mux.Handle(alarmPath, alarmHandler)
	return h2c.NewHandler(mux, &http2.Server{})
}

// NewHTTPServer applies bounded timeouts and header limits.
func NewHTTPServer(addr string, handler http.Handler) *http.Server {
	return &http.Server{
		Addr:              addr,
		Handler:           handler,
		ReadHeaderTimeout: 5 * time.Second,
		ReadTimeout:       15 * time.Second,
		WriteTimeout:      30 * time.Second,
		IdleTimeout:       120 * time.Second,
		MaxHeaderBytes:    16 << 10,
	}
}
