.PHONY: license
license: ## Add BUSL-1.1 license headers using addlicense
	@echo "Adding BUSL-1.1 license headers..."
	@command -v addlicense >/dev/null 2>&1 || (echo "addlicense not found. Install with: make install-addlicense" && exit 1)
	@addlicense -f LICENSE-HEADER.txt \
		-ignore "target/**" \
		-ignore "**/vendor/**" \
		.

.PHONY: license-check
license-check: ## Check if all Rust files have license headers
	@echo "Checking BUSL-1.1 license headers..."
	@command -v addlicense >/dev/null 2>&1 || (echo "addlicense not found. Install with: make install-addlicense" && exit 1)
	@addlicense -check -f LICENSE-HEADER.txt \
		-ignore "target/**" \
		-ignore "**/vendor/**" \
		. && echo "All files have license headers ✓" || \
		(echo "Some files are missing license headers. Run 'make license' to fix." && exit 1)

.PHONY: license-update
license-update: ## Update copyright year in headers
	@echo "Updating license headers with current year..."
	@find . -name "*.rs" -not -path "./target/*" -type f -exec sed -i 's/Copyright (c) [0-9]\{4\} M. Javani/Copyright (c) $(shell date +%Y) M. Javani/g' {} +
	@echo "License headers updated ✓"
