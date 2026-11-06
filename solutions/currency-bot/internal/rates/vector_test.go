package rates

import (
	"flag"
	"os"
	"testing"

	"github.com/proflyder/monorepo/solutions/currency-bot/internal/golden"
)

var updateGolden = flag.Bool("update", false, "rewrite golden vectors")

const ratesVectorPath = "../../../../solutions/currency-bot/testdata/vectors/rates.json"

type pairVector struct {
	Buy  float64 `json:"buy"`
	Sell float64 `json:"sell"`
}

type ratesVector struct {
	USD pairVector `json:"usd"`
	RUB pairVector `json:"rub"`
}

type parseCase struct {
	Name  string       `json:"name"`
	HTML  string       `json:"html"`
	Error bool         `json:"error,omitempty"`
	Rates *ratesVector `json:"rates,omitempty"`
}

type outlierCase struct {
	Name  string    `json:"name"`
	Input []float64 `json:"input"`
	Want  []float64 `json:"want"`
}

type bestAverageCase struct {
	Name      string    `json:"name"`
	Values    []float64 `json:"values"`
	Ascending bool      `json:"ascending"`
	Want      float64   `json:"want"`
}

type ratesVectors struct {
	ParseCases       []parseCase       `json:"parse_cases"`
	OutlierCases     []outlierCase     `json:"outlier_cases"`
	BestAverageCases []bestAverageCase `json:"best_average_cases"`
}

func TestRateVectors(t *testing.T) {
	liveHTML, err := os.ReadFile(golden.Resolve(t, "../../../../solutions/currency-bot/testdata/rates/kurskz.html"))
	if err != nil {
		t.Fatal(err)
	}

	parseCases := []parseCase{
		{Name: "live_fixture", HTML: string(liveHTML)},
		{Name: "missing_marker", HTML: "<html>no rates here</html>"},
		{Name: "data_object_skips_entry", HTML: `<script>var punkts = [{"name":"a","data":{}},{"name":"b","data":{"USD":[10,11],"RUB":[1,2]}}];</script>`},
		{Name: "zero_quote_dropped", HTML: `<script>var punkts = [{"data":{"USD":[0,0],"RUB":[0,0]}},{"data":{"USD":[10,11],"RUB":[1,2]}}];</script>`},
		{Name: "missing_rub_fails", HTML: `<script>var punkts = [{"data":{"USD":[10,11]}}];</script>`},
		{Name: "numeric_strings_accepted", HTML: `<script>var punkts = [{"data":{"USD":["10.5","11.5"],"RUB":["1.5","2.5"]}}];</script>`},
		{Name: "short_array_rejected", HTML: `<script>var punkts = [{"data":{"USD":[10],"RUB":[1,2]}}];</script>`},
	}
	for index := range parseCases {
		rates, err := Parse(parseCases[index].HTML, nil)
		if err != nil {
			parseCases[index].Error = true
			parseCases[index].Rates = nil
			continue
		}
		parseCases[index].Rates = &ratesVector{
			USD: pairVector{Buy: rates.USDToKZT.Buy, Sell: rates.USDToKZT.Sell},
			RUB: pairVector{Buy: rates.RUBToKZT.Buy, Sell: rates.RUBToKZT.Sell},
		}
	}

	outliers := []outlierCase{
		{Name: "keeps_small_samples", Input: []float64{1, 2, 3, 5000}},
		{Name: "drops_typo", Input: []float64{100, 101, 102, 103, 5000, 104, 105, 106, 107, 108}},
	}
	for index := range outliers {
		outliers[index].Want = removeOutliers(outliers[index].Input, outliers[index].Name, nil)
	}

	averages := []bestAverageCase{
		{Name: "ascending_top_five", Values: []float64{1, 2, 3, 4, 5, 6, 7}, Ascending: true},
		{Name: "descending_top_five", Values: []float64{1, 2, 3, 4, 5, 6, 7}, Ascending: false},
		{Name: "fewer_than_five", Values: []float64{10, 20}, Ascending: true},
	}
	for index := range averages {
		averages[index].Want = bestAverage(averages[index].Values, averages[index].Ascending)
	}

	golden.Run(t, *updateGolden, ratesVectorPath, ratesVectors{
		ParseCases:       parseCases,
		OutlierCases:     outliers,
		BestAverageCases: averages,
	})
}
