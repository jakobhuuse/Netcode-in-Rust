.PHONY: all test clean lint format check bench install help run-server run-client

# Default target
all: check test

# Colors for output
GREEN := \033[0;32m
YELLOW := \033[0;33m
BLUE := \033[0;34m
NC := \033[0m # No Color

help: ## Show this help message
	@echo "Available targets:"
	@awk 'BEGIN {FS = ":.*?## "}; /^[a-zA-Z_-]+:.*?## / {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

# Development commands
check: ## Check code without running tests
	@echo "$(BLUE)🔍 Checking code...$(NC)"
	@cargo check --workspace --all-targets --all-features

lint: ## Run clippy linter
	@echo "$(BLUE)🔍 Running clippy...$(NC)"
	@cargo clippy --workspace --all-targets --all-features -- -D warnings

format: ## Format code
	@echo "$(BLUE)🎨 Formatting code...$(NC)"
	@cargo fmt --all

format-check: ## Check if code is formatted
	@echo "$(BLUE)🎨 Checking code formatting...$(NC)"
	@cargo fmt --all -- --check

# Testing commands
test: ## Run all tests
	@echo "$(GREEN)🚀 Running all tests...$(NC)"
	@echo "=========================="
	@$(MAKE) test-shared
	@$(MAKE) test-server  
	@$(MAKE) test-client
	@$(MAKE) test-integration
	@echo "$(GREEN)✅ All tests completed!$(NC)"

test-shared: ## Test shared library
	@echo "$(YELLOW)📦 Testing shared library...$(NC)"
	@cargo test -p shared --lib

test-server: ## Test server components
	@echo "$(YELLOW)🖥️  Testing server components...$(NC)"
	@cd server && cargo test

test-client: ## Test client components  
	@echo "$(YELLOW)💻 Testing client components...$(NC)"
	@cd client && cargo test

test-integration: ## Run integration tests
	@echo "$(YELLOW)🔗 Running integration tests...$(NC)"
	@cargo test --test integration_tests

# Benchmark commands
bench: ## Run benchmark tests
	@echo "$(YELLOW)⚡ Running benchmark tests...$(NC)"
	@cargo test --test benchmark_tests -- --nocapture

bench-release: ## Run benchmarks in release mode (more accurate)
	@echo "$(YELLOW)⚡ Running release benchmarks...$(NC)"
	@cargo test --release --test benchmark_tests -- --nocapture

# Build commands  
build: ## Build all packages
	@echo "$(BLUE)🔨 Building all packages...$(NC)"
	@cargo build --workspace

build-release: ## Build all packages in release mode
	@echo "$(BLUE)🔨 Building release packages...$(NC)" 
	@cargo build --workspace --release

# Run commands
run-server: ## Run the server (use ARGS="--flag" for custom flags)
	@echo "$(GREEN)🖥️  Starting server...$(NC)"
	@cargo run -p server -- $(ARGS)

run-client: ## Run the client (use ARGS="--flag" for custom flags)
	@echo "$(GREEN)💻 Starting client...$(NC)"
	@cargo run -p client -- $(ARGS)

# Utility commands
clean: ## Clean build artifacts
	@echo "$(BLUE)🧹 Cleaning build artifacts...$(NC)"
	@cargo clean

install: ## Install development dependencies
	@echo "$(BLUE)📦 Installing development tools...$(NC)"
	@rustup component add clippy rustfmt
	@$(MAKE) coverage-install