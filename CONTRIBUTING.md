# Contributing to Ted

Thank you for your interest in contributing to Ted! We welcome contributions from the community.

## Getting Started

1. **Fork the Repository**
   - Fork the Ted repository on GitHub
   - Clone your forked repository locally

2. **Set Up Development Environment**
   ```bash
   # Clone the repository
   git clone https://github.com/your-username/ted.git
   cd ted

   # Install development tools
   cargo install cargo-make
   cargo make dev  # Run in development mode
   ```

## Development Workflow

### Prerequisites
- Rust (latest stable version)
- cargo-make (`cargo install cargo-make`)
- An Anthropic API key for testing

### Common Tasks
```bash
# Run development build
cargo make dev

# Run tests
cargo make test

# Run linters
cargo make lint

# Generate coverage report
cargo make cov-html
```

## Contributing Code

1. **Create a Branch**
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Code Guidelines**
   - Follow Rust's official style guidelines
   - Run `cargo fmt` before committing
   - Ensure `cargo clippy` passes with no warnings
   - Write tests for new functionality
   - Add documentation comments

3. **Commit Messages**
   - Use clear, descriptive commit messages
   - Follow conventional commits format:
     - `feat:` for new features
     - `fix:` for bug fixes
     - `docs:` for documentation changes
     - `refactor:` for code restructuring
     - `test:` for test-related changes

## Submitting a Pull Request

1. Ensure all tests pass: `cargo make ci`
2. Push your branch to your fork
3. Open a pull request against the main repository
4. Describe the purpose and implementation of your changes

## Reporting Issues

- Use GitHub Issues
- Provide a clear description
- Include reproduction steps
- Share relevant code snippets or error messages
- Specify your Ted version and environment

## Code of Conduct

Please review our [Code of Conduct](CODE_OF_CONDUCT.md) before contributing.

## Questions?

- Open a GitHub Discussion
- Join our community channels (to be added)

Thank you for contributing to Ted! 🚀