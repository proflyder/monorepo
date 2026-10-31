package server

import (
	"encoding/json"
	"net/http"
	"time"

	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
)

type exchangeRateDTO struct {
	Buy  float64 `json:"buy"`
	Sell float64 `json:"sell"`
}

type ratesDTO struct {
	USDToKZT exchangeRateDTO `json:"usdToKzt"`
	RUBToKZT exchangeRateDTO `json:"rubToKzt"`
}

type recordDTO struct {
	Timestamp string   `json:"timestamp"`
	Rates     ratesDTO `json:"rates"`
}

type historyResponse struct {
	Records    []recordDTO `json:"records"`
	TotalCount int         `json:"totalCount"`
}

type latestResponse struct {
	Timestamp string   `json:"timestamp"`
	Rates     ratesDTO `json:"rates"`
}

type deleteResponse struct {
	DeletedCount int    `json:"deletedCount"`
	Message      string `json:"message"`
}

func toRatesDTO(rates model.Rates) ratesDTO {
	return ratesDTO{
		USDToKZT: exchangeRateDTO{Buy: rates.USDToKZT.Buy, Sell: rates.USDToKZT.Sell},
		RUBToKZT: exchangeRateDTO{Buy: rates.RUBToKZT.Buy, Sell: rates.RUBToKZT.Sell},
	}
}

func toRecordDTO(record model.Record) recordDTO {
	return recordDTO{
		Timestamp: record.Timestamp.UTC().Format(time.RFC3339Nano),
		Rates:     toRatesDTO(record.Rates),
	}
}

// handleHistory implements GET /api/history. Precedence: date wins; otherwise
// from+to; from-only or to-only silently fall back to the full history.
func (s *Server) handleHistory(w http.ResponseWriter, r *http.Request) {
	query := r.URL.Query()
	dateParam := query.Get("date")
	fromParam := query.Get("from")
	toParam := query.Get("to")

	var from, to *time.Time
	switch {
	case dateParam != "":
		day, err := time.Parse("2006-01-02", dateParam)
		if err != nil {
			s.writeError(w, r, http.StatusBadRequest, "Bad Request",
				"Invalid date: expected YYYY-MM-DD", "BAD_REQUEST", "CurrencyHistoryController.getHistory", nil)
			return
		}
		start := day.UTC()
		end := start.Add(24 * time.Hour)
		from, to = &start, &end
	case fromParam != "" && toParam != "":
		start, err := time.Parse(time.RFC3339, fromParam)
		if err != nil {
			s.writeError(w, r, http.StatusBadRequest, "Bad Request",
				"Invalid from: expected RFC3339", "BAD_REQUEST", "CurrencyHistoryController.getHistory", nil)
			return
		}
		end, err := time.Parse(time.RFC3339, toParam)
		if err != nil {
			s.writeError(w, r, http.StatusBadRequest, "Bad Request",
				"Invalid to: expected RFC3339", "BAD_REQUEST", "CurrencyHistoryController.getHistory", nil)
			return
		}
		from, to = &start, &end
	}

	var (
		records []model.Record
		err     error
	)
	if from != nil && to != nil {
		records, err = s.opts.History.RecordsByDateRange(r.Context(), *from, *to)
	} else {
		records, err = s.opts.History.AllRecords(r.Context())
	}
	if err != nil {
		s.writeError(w, r, http.StatusInternalServerError, "Internal Server Error",
			"Failed to fetch currency history: "+err.Error(), "DATABASE_ERROR", "CurrencyHistoryController.getHistory", nil)
		return
	}
	out := historyResponse{Records: make([]recordDTO, 0, len(records)), TotalCount: len(records)}
	for _, record := range records {
		out.Records = append(out.Records, toRecordDTO(record))
	}
	writeJSON(w, http.StatusOK, out)
}

// handleLatest implements GET /api/latest.
func (s *Server) handleLatest(w http.ResponseWriter, r *http.Request) {
	record, err := s.opts.History.LatestRecord(r.Context())
	if isNotFound(err) {
		s.writeError(w, r, http.StatusNotFound, "Not Found",
			"No currency rates found in database", "NOT_FOUND", "CurrencyHistoryController.getLatest", nil)
		return
	}
	if err != nil {
		s.writeError(w, r, http.StatusInternalServerError, "Internal Server Error",
			"Failed to fetch latest currency rate: "+err.Error(), "DATABASE_ERROR", "CurrencyHistoryController.getLatest", nil)
		return
	}
	writeJSON(w, http.StatusOK, latestResponse{
		Timestamp: record.Timestamp.UTC().Format(time.RFC3339Nano),
		Rates:     toRatesDTO(record.Rates),
	})
}

// handleDeleteHistory implements DELETE /api/history.
func (s *Server) handleDeleteHistory(w http.ResponseWriter, r *http.Request) {
	deleted, err := s.opts.History.DeleteAll(r.Context())
	if err != nil {
		s.writeError(w, r, http.StatusInternalServerError, "Internal Server Error",
			"Failed to delete currency history: "+err.Error(), "DATABASE_ERROR", "CurrencyHistoryController.deleteHistory", nil)
		return
	}
	writeJSON(w, http.StatusOK, deleteResponse{
		DeletedCount: deleted,
		Message:      "Successfully deleted " + itoa(deleted) + " currency history records",
	})
}

type triggerRequest struct {
	ChatID *string `json:"chatId"`
}

type triggerResponse struct {
	Message         string `json:"message"`
	ExecutionTimeMs int64  `json:"executionTimeMs"`
}

// handleTrigger implements POST /api/trigger. The pipeline runs in-process;
// forceNotification=true and saveToHistory=false are the Kotlin contract.
func (s *Server) handleTrigger(w http.ResponseWriter, r *http.Request) {
	var request triggerRequest
	decoder := json.NewDecoder(http.MaxBytesReader(w, r.Body, 64*1024))
	if err := decoder.Decode(&request); err != nil {
		// An unparseable body is silently ignored, exactly like Kotlin.
		request = triggerRequest{}
	}
	chatID := s.opts.DefaultChatID
	if request.ChatID != nil && *request.ChatID != "" {
		chatID = *request.ChatID
	}

	start := time.Now()
	err := s.opts.Pipeline.Run(r.Context(), "trigger", chatID, true, false)
	elapsed := time.Since(start).Milliseconds()
	if err != nil {
		s.writeError(w, r, http.StatusBadGateway, "Bad Gateway",
			"Failed to update and send currency rates: "+err.Error(), "EXTERNAL_SERVICE_ERROR",
			"TriggerController.triggerCurrencyUpdate",
			map[string]string{"chatId": chatID, "executionTimeMs": itoa64(elapsed)})
		return
	}
	writeJSON(w, http.StatusOK, triggerResponse{
		Message:         "Currency rates updated and sent to Telegram successfully",
		ExecutionTimeMs: elapsed,
	})
}
