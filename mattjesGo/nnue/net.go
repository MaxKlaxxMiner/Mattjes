// Package nnue is a native port of Patricia's evaluation network (Patricia
// 5.1 by Adam Kulju, MIT licence, nets/LICENSE-Patricia): the three nets
// are embedded, the accumulator is updated incrementally per move, and the
// output is Patricia's raw score for the side to move. Nothing here runs
// an external engine; the reference values for the verification come from
// a local build of Patricia with an added "eval" command
// (tests/patricia-eval.sh).
//
// The architecture is (768 -> 1024)x2 -> 1 with SCReLU: 768 inputs are
// "piece of colour c of kind k on square s", one hidden layer of 1024
// neurons per perspective (one accumulator for white's view, one for
// black's, the latter with the board flipped so that the net always sees
// "my pieces" the same way), and one output neuron that reads the side to
// move's accumulator first and the opponent's second. Quantised: the
// feature weights and biases are int16 with scale QA = 255, the output
// weights int16 with scale QB = 64, the activation is SCReLU
// (clamp(x, 0, QA) squared), and the result is scaled to centipawns with
// SCALE = 400. Patricia keeps three nets and picks one by the material on
// the board (PhaseOf): the "sacrifice" net above PhaseBound, the endgame
// net below it; the middlegame net is used by its search when it is
// behind.
package nnue

import (
	_ "embed"
	"encoding/binary"
	"math/bits"
	"sync"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

const (
	InputSize  = 768  // 2 colours x 6 kinds x 64 squares
	HiddenSize = 1024 // neurons per perspective
	QA         = 255  // scale of the feature layer (and the SCReLU clamp)
	QB         = 64   // scale of the output layer
	Scale      = 400  // centipawn scale of the output
	// PhaseBound is the total material (Material) below which Patricia
	// switches to the endgame net.
	PhaseBound = 3500
)

// Phase selects one of the three nets.
type Phase uint8

const (
	Middlegame Phase = iota // fingolfin.nnue: Patricia's search uses it when it is behind
	Endgame                 // finarfin.nnue: material below PhaseBound
	Sacrifice               // feanor.nnue: the default net above PhaseBound
	phaseCount
)

func (p Phase) String() string { return [...]string{"middlegame", "endgame", "sacrifice"}[p] }

// Net holds one network's parameters in Patricia's memory layout: the
// feature weights row by row (one row of HiddenSize per input feature), the
// hidden biases, the output weights (the side to move's half first), the
// output bias. The file is exactly these arrays as little-endian int16.
type Net struct {
	Feature    []int16 // InputSize * HiddenSize
	Bias       []int16 // HiddenSize
	Output     []int16 // 2 * HiddenSize
	OutputBias int16
}

// FileBytes is the size of a net file.
const FileBytes = (InputSize*HiddenSize + HiddenSize + 2*HiddenSize + 1) * 2

//go:embed nets/fingolfin.nnue
var fingolfin []byte

//go:embed nets/finarfin.nnue
var finarfin []byte

//go:embed nets/feanor.nnue
var feanor []byte

var (
	loadOnce sync.Once
	nets     [phaseCount]*Net
)

// Load returns the net of a phase (parsed from the embedded file on first
// use, about 5 MB for all three).
func Load(phase Phase) *Net {
	loadOnce.Do(func() {
		for p, raw := range [phaseCount][]byte{fingolfin, finarfin, feanor} {
			nets[p] = parse(raw)
		}
	})
	return nets[phase]
}

func parse(raw []byte) *Net {
	if len(raw) != FileBytes {
		panic("nnue: net file has the wrong size")
	}
	values := make([]int16, len(raw)/2)
	for i := range values {
		values[i] = int16(binary.LittleEndian.Uint16(raw[2*i:]))
	}
	n := &Net{}
	n.Feature, values = values[:InputSize*HiddenSize], values[InputSize*HiddenSize:]
	n.Bias, values = values[:HiddenSize], values[HiddenSize:]
	n.Output, values = values[:2*HiddenSize], values[2*HiddenSize:]
	n.OutputBias = values[0]
	return n
}

// Row is the feature weights of one input feature.
func (n *Net) Row(feature int) []int16 {
	return n.Feature[feature*HiddenSize : (feature+1)*HiddenSize]
}

// patriciaKind maps a bitboard kind index (king 0, queen 1, rook 2, bishop
// 3, knight 4, pawn 5) to Patricia's (pawn 0, knight 1, bishop 2, rook 3,
// queen 4, king 5): the reverse order.
func patriciaKind(kind int) int { return 5 - kind }

// kindIdx is the bitboard kind index of a piece (trailing zero of its type bit).
func kindIdx(p chess.Piece) int { return bits.TrailingZeros8(uint8(p & chess.TypeMask)) }

// FeatureIndices returns the input feature of a piece on a square from
// white's and from black's perspective. Patricia numbers the squares
// a1 = 0 .. h8 = 63 (rank first), Mattjes a8 = 0 .. h1 = 63, so Patricia's
// square is pos ^ 56; black's perspective flips the board vertically
// (square ^ 56 again, so it is the Mattjes number) and swaps the colours.
func FeatureIndices(p chess.Piece, sq chess.Pos) (white, black int) {
	kind := patriciaKind(kindIdx(p))
	colour := 0
	if p&chess.ColorMask == chess.Black {
		colour = 1
	}
	psq := int(sq) ^ 56
	white = colour*384 + kind*64 + psq
	black = (colour^1)*384 + kind*64 + (psq ^ 56)
	return white, black
}

// Material is Patricia's total material of both sides (pawn 100, knight
// and bishop 300, rook 500, queen 900), the phase criterion.
func Material(b *bitboard.Board) int {
	total := 0
	for side := 0; side < 2; side++ {
		for kind, value := range [6]int{0, 900, 500, 300, 300, 100} {
			total += value * bits.OnesCount64(b.Pieces[side][kind])
		}
	}
	return total
}

// PhaseOf is the net Patricia starts a search with: the endgame net below
// PhaseBound, the sacrifice net above (the middlegame net only when its
// search finds itself behind, which a static evaluation cannot know).
func PhaseOf(b *bitboard.Board) Phase {
	if Material(b) < PhaseBound {
		return Endgame
	}
	return Sacrifice
}
