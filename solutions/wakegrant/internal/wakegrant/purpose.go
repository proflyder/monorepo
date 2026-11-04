package wakegrant

// Purpose distinguishes what a grant may do. A logon grant is only valid on a
// cold boot (the Windows provider does not claim it while a user session
// exists); an unlock grant is for a running but locked PC.
type Purpose string

const (
	PurposeLogon  Purpose = "logon"
	PurposeUnlock Purpose = "unlock"
)

// DefaultPurpose is what a request without an explicit "purpose" means,
// matching wakegrant v1.
const DefaultPurpose = PurposeLogon

// ParsePurpose validates an explicit purpose field.
func ParsePurpose(value string) (Purpose, bool) {
	switch Purpose(value) {
	case PurposeLogon:
		return PurposeLogon, true
	case PurposeUnlock:
		return PurposeUnlock, true
	default:
		return "", false
	}
}

// Long-poll bounds for POST /v1/claim with a purpose.
const (
	MinWaitSeconds     = 1
	MaxWaitSeconds     = 25
	MaxWaitersPerQueue = 2
)
