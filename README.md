# Mattjes

A chess engine specialized in **mate search and draw search**, written in Go and Rust.

> **Status: experimental / early development.**
> Mattjes is a learning and research project. It is being rebuilt from scratch after a
> five-year pause. There is no playable engine yet, no UCI support, and the public API
> changes without notice. Expect rough edges, dead ends and benchmark code in `main`.

## Goal

Classic engines aim for playing strength. Mattjes does not. It focuses on answering
two narrow questions as fast and as reliably as possible:

- **Is there a forced mate, and in how many moves?**
- **Is this position a guaranteed draw?**

That focus allows approaches that would be unusual in a general-purpose engine:
list-based search instead of recursion, proof-number style algorithms instead of
plain alpha/beta, transposition tables with longer, collision-resistant keys, and
retrograde analysis of mate positions.

## Two languages, one algorithm

The project is deliberately developed in **Go and Rust in parallel**:

| Directory    | Language | Role                                                        |
|--------------|----------|-------------------------------------------------------------|
| `mattjesGo/` | Go       | Primary implementation, algorithms are prototyped here      |
| `mattjesRs/` | Rust     | Port of the same algorithms, cross-checked against the Go version for correctness (perft) and speed |
| `old/`       | C#       | The original 2019-2021 prototype, kept for reference only (test positions, ideas). Not maintained, not buildable. |

Every move generator, hash scheme or search approach lives in its own package so
that experiments can be dropped again without touching the rest.

## Building

The toolchain is expected to run under MSYS2/ucrt64 (Go 1.26, Rust 1.95 from pacman),
but plain Go and Cargo installations on any platform should work as well.

```sh
./build.sh        # builds both -> runMattjesGo.exe and runMattjesRs.exe in the repo root
./build.sh go     # Go only
./build.sh rs     # Rust only (release profile)

./runMattjesGo.exe
./runMattjesRs.exe
```

Both binaries are plain console programs. Which test or benchmark runs is selected
by commenting code in and out in `main`. There is no CLI, no configuration and no
test framework at this stage; that is intentional while the fundamentals are explored.

## Roadmap

1. **Move generator** in Go (minimalistic mailbox design, based on the author's
   earlier [yacboard](https://github.com/MaxKlaxxMiner/huschiBoard) generator), verified
   with perft. Both a classic recursive and a list-based variant, measured for speed
   and memory. Then a Rust port, then bitboard-based generators as alternatives.
2. **Hash keys**: CRC64, Zobrist, 128-bit keys and full 256-bit collision-free keys compared.
3. **Transposition tables** with different key sizes and optional persistence.
4. **Mate and draw search**, possibly two-staged: a fast pre-search for likely results,
   then an exact search for the optimal move count.
5. **Endgame tablebases** (Syzygy), preferably a native implementation rather than a library.
6. **UCI** support, so the engine can be used in GUIs such as Arena or Cute Chess.
7. Optional: **neural networks** to find winning positions faster.

## License

Mattjes is free software, licensed under the **GNU General Public License v3.0 or later**.
See [LICENSE](LICENSE). Ideas from other GPL engines (Stockfish, Reckless and others)
may be adopted where they fit the goal.
