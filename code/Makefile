QUINT_VERSION := $(shell cat ../specs/quint-version)

.PHONY: help install install-quint lint lint-fix integration-tests discovery-tests tests

help: ## Show this help.
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage: make \033[36m\033[0m\n"} /^[$$()% a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-20s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

install: install-quint ## Install the required tools.
	cargo install --locked cargo-nextest

install-quint: ## Install the Quint CLI version used by CI.
	npm install --global "@informalsystems/quint@$(QUINT_VERSION)"
	@actual=$$(quint --version) && echo "Quint $$actual" && test "$$actual" = "$(QUINT_VERSION)" || \
	  { echo "Expected Quint $(QUINT_VERSION) on PATH" >&2; exit 1; }

lint: ## Lint the workspace.
	cargo fmt --all --check
	cargo clippy --workspace --all-features --all-targets -- -D warnings

lint-fix: ## Fix the linting issues.
	cargo clippy --fix --allow-dirty --allow-staged --workspace --all-features --all-targets -- -D warnings
	cargo fmt --all

integration-tests: ## Run the integration tests.
	cargo nextest run \
		--workspace \
		--all-features \
		--no-fail-fast \
		--failure-output final \
		--test-threads 1 \
		--exclude arc-malachitebft-discovery-test

discovery-tests: ## Run the discovery tests
	cargo nextest run \
		--all-features \
		--no-fail-fast \
		--failure-output final \
		--test-threads 1 \
		--package arc-malachitebft-discovery-test

tests: ## Run all the tests.
	$(MAKE) integration-tests
	$(MAKE) discovery-tests
