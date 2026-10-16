package egtb

import (
	"sort"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// MissingTable is a material without a table that a search asked for, with
// the number of lookups.
type MissingTable struct {
	Mat    Material
	Probes uint64
}

// MissingTables lists the materials without a table that lookups asked for
// since the last reset, most asked first, at most n (n <= 0: all). That is
// the demand a search creates: the table asked for most is the one worth
// generating next (docs/m6-uci.md, "Erzeugung nach Bedarf").
func (s *Set) MissingTables(n int) []MissingTable {
	var out []MissingTable
	s.missingMu.Lock()
	for sig, probes := range s.missing {
		m := materialFromSignature(sig)
		if t := s.Find(m.Name()); t != nil && t.available() {
			delete(s.missing, sig) // asked for before the table arrived: no demand any more
			continue
		}
		if probes > 0 {
			out = append(out, MissingTable{Mat: m, Probes: probes})
		}
	}
	s.missingMu.Unlock()
	sort.Slice(out, func(i, j int) bool {
		if out[i].Probes != out[j].Probes {
			return out[i].Probes > out[j].Probes
		}
		return out[i].Mat.Name() < out[j].Mat.Name()
	})
	if n > 0 && len(out) > n {
		out = out[:n]
	}
	return out
}

// ResetMissing zeroes the counters of MissingTables.
func (s *Set) ResetMissing() {
	s.missingMu.Lock()
	clear(s.missing)
	s.missingMu.Unlock()
}

// ForgetMissing drops one material from MissingTables (its generation was
// decided), so that the next decision looks at the rest.
func (s *Set) ForgetMissing(m Material) {
	s.missingMu.Lock()
	delete(s.missing, m.signature())
	s.missingMu.Unlock()
}

// countMissing is one lookup of a material without a table.
func (s *Set) countMissing(sig Signature) {
	s.missingMu.Lock()
	s.missing[sig]++
	s.missingMu.Unlock()
}

// materialFromSignature decodes the two-bit counts per type and colour (see
// boardSignature) into a canonical material.
func materialFromSignature(sig Signature) Material {
	var w, bl []chess.Piece
	for _, p := range []chess.Piece{chess.Pawn, chess.Knight, chess.Bishop, chess.Rook, chess.Queen} {
		for i := 0; i < int(sig>>typeBit(p))&3; i++ {
			w = append(w, p)
		}
		for i := 0; i < int(sig>>(typeBit(p)+10))&3; i++ {
			bl = append(bl, p)
		}
	}
	return canonicalMaterial(w, bl)
}
