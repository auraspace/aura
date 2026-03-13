FROM rust:latest

# Install dependencies for the Aura compiler backend
RUN apt-get update && apt-get install -y \
    gcc \
    libc6-dev \
    libpthread-stubs0-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the entire workspace
COPY . .

# Build the Aura compiler
RUN cargo build

# Run E2E tests by default
CMD ["cargo", "test", "--test", "e2e", "--", "interp,compiler"]
