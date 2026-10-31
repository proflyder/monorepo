package rates

import (
	"math"
	"os"
	"testing"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

func TestParseLiveFixture(t *testing.T) {
	html, err := os.ReadFile("testdata/kurskz.html")
	if err != nil {
		t.Fatal(err)
	}
	got, err := Parse(string(html), nil)
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	// Independently hand-computed from the fixture values:
	// USD buy IQR removes the three 452 lows; top-5 = 454×4 + 453.8.
	// USD sell top-5 ascending = 454.5+454.5+454.7×3.
	// RUB buy top-5 descending = 5.38×4 + 5.37.
	// RUB sell bottom-5 = 5.42×2 + 5.43×3.
	assertClose(t, "USD buy", got.USDToKZT.Buy, 453.9)
	assertClose(t, "USD sell", got.USDToKZT.Sell, 454.62)
	assertClose(t, "RUB buy", got.RUBToKZT.Buy, 5.378)
	assertClose(t, "RUB sell", got.RUBToKZT.Sell, 5.426)
}

func TestParseEdgeCases(t *testing.T) {
	tests := []struct {
		name    string
		html    string
		wantErr bool
		check   func(t *testing.T, got model.Rates)
	}{
		{
			name:    "missing marker",
			html:    `<html>no rates here</html>`,
			wantErr: true,
		},
		{
			name: "data object skips entry",
			html: `<script>var punkts = [{"name":"a","data":{}},{"name":"b","data":{"USD":[10,11],"RUB":[1,2]}}];</script>`,
			check: func(t *testing.T, got model.Rates) {
				assertClose(t, "usd buy", got.USDToKZT.Buy, 10)
				assertClose(t, "rub sell", got.RUBToKZT.Sell, 2)
			},
		},
		{
			name: "zero quote dropped",
			html: `<script>var punkts = [{"data":{"USD":[0,0],"RUB":[0,0]}},{"data":{"USD":[10,11],"RUB":[1,2]}}];</script>`,
			check: func(t *testing.T, got model.Rates) {
				assertClose(t, "usd sell", got.USDToKZT.Sell, 11)
				assertClose(t, "rub buy", got.RUBToKZT.Buy, 1)
			},
		},
		{
			name:    "missing rub fails whole parse",
			html:    `<script>var punkts = [{"data":{"USD":[10,11]}}];</script>`,
			wantErr: true,
		},
		{
			name: "numeric strings accepted",
			html: `<script>var punkts = [{"data":{"USD":["10.5","11.5"],"RUB":["1.5","2.5"]}}];</script>`,
			check: func(t *testing.T, got model.Rates) {
				assertClose(t, "usd buy", got.USDToKZT.Buy, 10.5)
				assertClose(t, "rub sell", got.RUBToKZT.Sell, 2.5)
			},
		},
		{
			name:    "short array rejected",
			html:    `<script>var punkts = [{"data":{"USD":[10],"RUB":[1,2]}}];</script>`,
			wantErr: true,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got, err := Parse(tc.html, nil)
			if tc.wantErr {
				if err == nil {
					t.Fatal("expected error")
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if tc.check != nil {
				tc.check(t, got)
			}
		})
	}
}

func TestRemoveOutliersKeepsSmallSamples(t *testing.T) {
	in := []float64{1, 2, 3, 5000}
	got := removeOutliers(in, "USD buy", nil)
	if len(got) != 4 {
		t.Fatalf("n=4 drops only the low tail, got %v", got)
	}
	// With n=4, q1=sorted[1]=2 and q3=sorted[3]=5000, so 5000 is never removed.
	for i := range in {
		if got[i] != in[i] {
			t.Fatalf("unexpected filtering: %v", got)
		}
	}
}

func TestRemoveOutliersDropsTypo(t *testing.T) {
	in := []float64{100, 101, 102, 103, 5000, 104, 105, 106, 107, 108}
	got := removeOutliers(in, "USD buy", nil)
	for _, value := range got {
		if value == 5000 {
			t.Fatalf("5000 should be removed, got %v", got)
		}
	}
}

func TestBestAverageUsesFive(t *testing.T) {
	values := []float64{1, 2, 3, 4, 5, 6, 7}
	if got := bestAverage(values, true); got != 3 { // (1+2+3+4+5)/5
		t.Fatalf("ascending average = %v, want 3", got)
	}
	if got := bestAverage(values, false); got != 5 { // (7+6+5+4+3)/5
		t.Fatalf("descending average = %v, want 5", got)
	}
}

func assertClose(t *testing.T, label string, got, want float64) {
	t.Helper()
	if math.Abs(got-want) > 1e-9 {
		t.Fatalf("%s = %v, want %v", label, got, want)
	}
}
