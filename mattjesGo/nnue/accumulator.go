package nnue

import (
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/bitboard"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/chess"
)

// Accumulator is the hidden layer before the activation, one vector per
// perspective: the bias plus the feature rows of every piece on the board.
// A move changes at most two features per side (castling: king and rook)
// and removes at most two (capture or en passant), so the accumulator of
// the next position is the old one plus and minus a few rows (Update)
// instead of a sum over all pieces (Refresh). The additions saturate at
// the int16 range, as Patricia's SIMD path does.
type Accumulator struct {
	White, Black [HiddenSize]int16
}

// Delta lists the features a move adds and removes, per perspective.
type Delta struct {
	AddWhite, AddBlack [2]uint16
	SubWhite, SubBlack [2]uint16
	Adds, Subs         uint8
}

func (d *Delta) add(p chess.Piece, sq chess.Pos) {
	w, b := FeatureIndices(p, sq)
	d.AddWhite[d.Adds], d.AddBlack[d.Adds] = uint16(w), uint16(b)
	d.Adds++
}

func (d *Delta) sub(p chess.Piece, sq chess.Pos) {
	w, b := FeatureIndices(p, sq)
	d.SubWhite[d.Subs], d.SubBlack[d.Subs] = uint16(w), uint16(b)
	d.Subs++
}

// DeltaOf is the feature change of move m on board b (b before the move):
// the moved piece leaves its square and arrives (as the promotion piece if
// any), a captured piece leaves (the en passant pawn from its own square),
// a castling rook moves too.
func DeltaOf(b *bitboard.Board, m chess.Move) Delta {
	var d Delta
	p := b.Squares[m.From]
	d.sub(p, m.From)
	if m.Capture != chess.None {
		d.sub(m.Capture, m.To)
	}
	if m.Promo != chess.None {
		d.add(m.Promo, m.To)
	} else {
		d.add(p, m.To)
	}
	if p.Is(chess.Pawn) {
		if m.To == b.EnPassant && m.From.X() != m.To.X() {
			if b.WhiteMove {
				d.sub(b.Squares[m.To+chess.Width], m.To+chess.Width)
			} else {
				d.sub(b.Squares[m.To-chess.Width], m.To-chess.Width)
			}
		}
	} else if p.Is(chess.King) {
		switch m.To - m.From {
		case 2: // kingside
			rook := b.Squares[m.To+1]
			d.sub(rook, m.To+1)
			d.add(rook, m.To-1)
		case -2: // queenside
			rook := b.Squares[m.To-2]
			d.sub(rook, m.To-2)
			d.add(rook, m.To+1)
		}
	}
	return d
}

// sat16 clamps to the int16 range. Go does not vectorise these loops: a
// refresh costs 46 us here against 4.5 us for the plain Rust loops that
// LLVM turns into vector code (the min/max builtins and a bounds-check
// hoist measured slower, 58 us; docs/nnue-patricia.md).
func sat16(v int32) int16 {
	if v > 32767 {
		return 32767
	}
	if v < -32768 {
		return -32768
	}
	return int16(v)
}

// Refresh computes both perspectives from scratch for board b.
func (a *Accumulator) Refresh(net *Net, b *bitboard.Board) {
	copy(a.White[:], net.Bias)
	copy(a.Black[:], net.Bias)
	for sq := chess.Pos(0); sq < chess.FieldCount; sq++ {
		p := b.Squares[sq]
		if p == chess.None {
			continue
		}
		w, bl := FeatureIndices(p, sq)
		addRow(&a.White, net.Row(w))
		addRow(&a.Black, net.Row(bl))
	}
}

func addRow(acc *[HiddenSize]int16, row []int16) {
	for i := range acc {
		acc[i] = sat16(int32(acc[i]) + int32(row[i]))
	}
}

func subRow(acc *[HiddenSize]int16, row []int16) {
	for i := range acc {
		acc[i] = sat16(int32(acc[i]) - int32(row[i]))
	}
}

// Update sets a to src plus the delta (the additions first, then the
// subtractions, each saturating, as Patricia's SIMD path does).
func (a *Accumulator) Update(net *Net, src *Accumulator, d *Delta) {
	a.White, a.Black = src.White, src.Black
	for i := 0; i < int(d.Adds); i++ {
		addRow(&a.White, net.Row(int(d.AddWhite[i])))
		addRow(&a.Black, net.Row(int(d.AddBlack[i])))
	}
	for i := 0; i < int(d.Subs); i++ {
		subRow(&a.White, net.Row(int(d.SubWhite[i])))
		subRow(&a.Black, net.Row(int(d.SubBlack[i])))
	}
}

func screlu(x int16) int64 {
	v := int64(x)
	if v < 0 {
		v = 0
	} else if v > QA {
		v = QA
	}
	return v * v
}

// Evaluate is the network's output for the side to move in centipawns
// (Patricia's raw score before its eval() scaling): the side to move's
// perspective through the first half of the output weights, the
// opponent's through the second.
func (a *Accumulator) Evaluate(net *Net, whiteToMove bool) int {
	us, them := &a.White, &a.Black
	if !whiteToMove {
		us, them = &a.Black, &a.White
	}
	var sum int64
	for i := 0; i < HiddenSize; i++ {
		sum += screlu(us[i]) * int64(net.Output[i])
		sum += screlu(them[i]) * int64(net.Output[HiddenSize+i])
	}
	biased := sum + int64(net.OutputBias)*QA
	return int((biased * Scale) / (int64(QA) * int64(QA*QB)))
}

// Raw evaluates board b from scratch with the net of a phase.
func Raw(b *bitboard.Board, phase Phase) int {
	var acc Accumulator
	net := Load(phase)
	acc.Refresh(net, b)
	return acc.Evaluate(net, b.WhiteMove)
}

// Evaluate is Patricia's static evaluation of a position as its search
// sees it at the root: the raw score of the net PhaseOf picks, scaled by
// the material ((750 + material/25)/1024, a tenth less when the side to
// move has no queen or the material is below 4000 and the score is in its
// favour). Patricia's search adds history-dependent terms on top
// (sacrifice bonuses, correction histories, the fifty-move scaling) that
// have no meaning for a single position.
func Evaluate(b *bitboard.Board) int {
	raw := Raw(b, PhaseOf(b))
	material := Material(b)
	// as in Patricia: material/25 in integers, the rest in float, the
	// tenth subtracted in double precision
	multiplier := float32(750+material/25) / 1024
	side := 0
	if !b.WhiteMove {
		side = 1
	}
	noQueen := b.Pieces[side][1] == 0
	if noQueen || material < 4000 && raw > 0 {
		multiplier = float32(float64(multiplier) - 0.1)
	}
	return int(float32(raw) * multiplier)
}
