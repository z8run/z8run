.PHONY: check fmt clippy build test frontend-check frontend-build ci clean

# ─── Quick checks (run before committing) ───────────────────────────
check: fmt clippy frontend-check
	@echo "✓ All checks passed"

# ─── Full CI pipeline (mirrors .github/workflows/ci.yml) ────────────
ci: fmt clippy build test frontend-check frontend-build
	@echo "✓ Full CI passed"

# ─── Rust ────────────────────────────────────────────────────────────
fmt:
	cargo fmt --all -- --check

fmt-fix:
	cargo fmt --all

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

build:
	cargo build --all

test:
	cargo test --all

# ─── Frontend ────────────────────────────────────────────────────────
frontend-check:
	cd frontend && npm run check

frontend-check-fix:
	cd frontend && npm run check:fix

frontend-build:
	cd frontend && npm run build

frontend-dev:
	cd frontend && npm run dev

# ─── Dev server ──────────────────────────────────────────────────────
serve:
	cargo run --bin z8run -- serve

serve-release:
	cargo run --release --bin z8run -- serve

# ─── Fix everything ─────────────────────────────────────────────────
fix: fmt-fix frontend-check-fix
	@echo "✓ All auto-fixes applied"

# ─── Clean ───────────────────────────────────────────────────────────
clean:
	cargo clean
	rm -rf frontend/dist
