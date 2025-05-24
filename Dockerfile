# Multi-stage build for optimized image size
FROM rust:latest AS builder

WORKDIR /app

# Copy everything and build
COPY . .

# Build the server
RUN cargo build --release -p server

# Runtime stage - use bookworm (Debian 12)
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -r -s /bin/false gameserver

# Copy binary from builder stage
COPY --from=builder /app/target/release/server /usr/local/bin/server

# Set ownership and permissions
RUN chown gameserver:gameserver /usr/local/bin/server

# Switch to non-root user
USER gameserver

# Expose the game port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD timeout 3 bash -c '</dev/tcp/localhost/8080' || exit 1

# Run the server
CMD ["server", "--host", "0.0.0.0", "--port", "8080", "--tick-rate", "60", "--max-clients", "32"]