check:
	cargo clippy --profile perf --all-targets -- -D warnings
	cargo test --profile perf
	gitleaks git --staged --no-banner

app:
	cargo build --profile perf
	scripts/make_app.sh
