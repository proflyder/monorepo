// Package rates fetches and aggregates exchange rates from kurs.kz. It
// replicates the Kotlin parser: regex extraction of the `var punkts = [...]`
// literal, IQR outlier removal and best-5 averaging. EUR and all other
// currencies are ignored; only USD and RUB are used.
package rates

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"math"
	"net/http"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

// DefaultURL is the live rates page.
const DefaultURL = "https://kurs.kz"

// punktsPattern mirrors the Kotlin regex, including the lazy match: the first
// `];` after the array opening bracket terminates the literal.
var punktsPattern = regexp.MustCompile(`(?s)var punkts = (\[.*?\]);`)

// Point is one exchange office's accepted USD or RUB quote.
type Point struct {
	Buy  float64
	Sell float64
}

// Fetcher downloads and parses kurs.kz.
type Fetcher struct {
	URL    string
	Client *http.Client
	Logger *slog.Logger
}

// NewFetcher returns a fetcher with sensible HTTP timeouts.
func NewFetcher(url string, client *http.Client, logger *slog.Logger) *Fetcher {
	if url == "" {
		url = DefaultURL
	}
	if client == nil {
		client = &http.Client{Timeout: 20_000_000_000}
	}
	if logger == nil {
		logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	return &Fetcher{URL: url, Client: client, Logger: logger}
}

// Fetch retrieves the page and parses it.
func (f *Fetcher) Fetch(ctx context.Context) (model.Rates, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, f.URL, nil)
	if err != nil {
		return model.Rates{}, err
	}
	// The Kotlin client sends no headers of its own; keep the default Go
	// User-Agent so the request is still a normal HTTP request.
	resp, err := f.Client.Do(req)
	if err != nil {
		return model.Rates{}, err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return model.Rates{}, err
	}
	f.Logger.Debug("received kurs.kz response", "bytes", len(body), "status", resp.StatusCode)
	return Parse(string(body), f.Logger)
}

// Parse extracts and aggregates rates from an HTML document.
func Parse(html string, logger *slog.Logger) (model.Rates, error) {
	if logger == nil {
		logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	match := punktsPattern.FindStringSubmatch(html)
	if match == nil {
		return model.Rates{}, fmt.Errorf("could not find 'var punkts' in HTML: %w", errParse)
	}

	var points []struct {
		Name string          `json:"name"`
		Data json.RawMessage `json:"data"`
	}
	if err := json.Unmarshal([]byte(match[1]), &points); err != nil {
		return model.Rates{}, fmt.Errorf("decode punkts: %w", err)
	}
	logger.Debug("parsed exchange points", "count", len(points))

	var usd, rub []Point
	for i, point := range points {
		var data map[string]json.RawMessage
		if len(point.Data) == 0 {
			continue
		}
		if err := json.Unmarshal(point.Data, &data); err != nil || data == nil {
			// data missing or not an object: skip silently, like Kotlin.
			continue
		}
		if p, ok := parsePoint(data["USD"]); ok {
			usd = append(usd, p)
			if i < 3 {
				logger.Info("exchange point quote", "name", nameOrUnknown(point.Name), "currency", "USD", "buy", p.Buy, "sell", p.Sell)
			}
		}
		if p, ok := parsePoint(data["RUB"]); ok {
			rub = append(rub, p)
			if i < 3 {
				logger.Info("exchange point quote", "name", nameOrUnknown(point.Name), "currency", "RUB", "buy", p.Buy, "sell", p.Sell)
			}
		}
	}
	logger.Info("collected exchange points", "currency", "USD", "count", len(usd))
	logger.Info("collected exchange points", "currency", "RUB", "count", len(rub))
	if len(usd) == 0 {
		return model.Rates{}, fmt.Errorf("no USD rates found: %w", errParse)
	}
	if len(rub) == 0 {
		return model.Rates{}, fmt.Errorf("no RUB rates found: %w", errParse)
	}

	usdBuy := bestAverage(removeOutliers(pluck(usd, func(p Point) float64 { return p.Buy }), "USD buy", logger), false)
	usdSell := bestAverage(removeOutliers(pluck(usd, func(p Point) float64 { return p.Sell }), "USD sell", logger), true)
	rubBuy := bestAverage(removeOutliers(pluck(rub, func(p Point) float64 { return p.Buy }), "RUB buy", logger), false)
	rubSell := bestAverage(removeOutliers(pluck(rub, func(p Point) float64 { return p.Sell }), "RUB sell", logger), true)

	return model.Rates{
		USDToKZT: model.ExchangeRate{Buy: usdBuy, Sell: usdSell},
		RUBToKZT: model.ExchangeRate{Buy: rubBuy, Sell: rubSell},
	}, nil
}

type parseError string

func (e parseError) Error() string { return string(e) }

const errParse parseError = "kurs.kz parse error"

func nameOrUnknown(name string) string {
	if name == "" {
		return "Unknown"
	}
	return name
}

func parsePoint(raw json.RawMessage) (Point, bool) {
	if len(raw) == 0 {
		return Point{}, false
	}
	var arr []json.RawMessage
	if err := json.Unmarshal(raw, &arr); err != nil || len(arr) < 2 {
		return Point{}, false
	}
	buy, ok := parseNumber(arr[0])
	if !ok || buy <= 0 {
		return Point{}, false
	}
	sell, ok := parseNumber(arr[1])
	if !ok || sell <= 0 {
		return Point{}, false
	}
	return Point{Buy: buy, Sell: sell}, true
}

// parseNumber accepts JSON numbers and numeric strings, matching kotlinx's
// JsonPrimitive.doubleOrNull. Empty strings, null and non-numeric values fail.
func parseNumber(raw json.RawMessage) (float64, bool) {
	s := strings.TrimSpace(string(raw))
	if s == "" || s == "null" {
		return 0, false
	}
	if strings.HasPrefix(s, `"`) {
		var str string
		if err := json.Unmarshal(raw, &str); err != nil {
			return 0, false
		}
		s = strings.TrimSpace(str)
		if s == "" {
			return 0, false
		}
	}
	v, err := strconv.ParseFloat(s, 64)
	if err != nil || math.IsNaN(v) || math.IsInf(v, 0) {
		return 0, false
	}
	return v, true
}

func pluck(points []Point, f func(Point) float64) []float64 {
	out := make([]float64, len(points))
	for i, p := range points {
		out[i] = f(p)
	}
	return out
}

// removeOutliers is the intentional non-textbook IQR filter from Kotlin:
// quartile indexes are size/4 and size*3/4 with no interpolation, and an empty
// filtered list falls back to the original values.
func removeOutliers(values []float64, label string, logger *slog.Logger) []float64 {
	if len(values) < 4 {
		return values
	}
	sorted := append([]float64(nil), values...)
	sort.Float64s(sorted)
	q1 := sorted[len(sorted)/4]
	q3 := sorted[len(sorted)*3/4]
	iqr := q3 - q1
	lower := q1 - 1.5*iqr
	upper := q3 + 1.5*iqr

	filtered := make([]float64, 0, len(values))
	for _, v := range values {
		if v >= lower && v <= upper {
			filtered = append(filtered, v)
		}
	}
	if len(filtered) < len(values) {
		if logger != nil {
			logger.Warn("removed outlier rates", "label", label, "removed", len(values)-len(filtered), "lower", lower, "upper", upper)
		}
	}
	if len(filtered) == 0 {
		return values
	}
	return filtered
}

// bestAverage takes the best five quotes: ascending for sell (cheapest rate
// when buying currency), descending for buy (highest rate when selling).
func bestAverage(values []float64, ascending bool) float64 {
	sorted := append([]float64(nil), values...)
	if ascending {
		sort.Float64s(sorted)
	} else {
		sort.Sort(sort.Reverse(sort.Float64Slice(sorted)))
	}
	n := 5
	if len(sorted) < n {
		n = len(sorted)
	}
	var sum float64
	for _, v := range sorted[:n] {
		sum += v
	}
	return sum / float64(n)
}
