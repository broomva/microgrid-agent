.PHONY: test simulate lint format deploy-rpi docker-build docker-run health

test:
	python -m pytest tests/ -v

simulate:
	python main.py --config config/site.example.toml --simulate

lint:
	ruff check src/ main.py tests/

format:
	ruff format src/ main.py tests/

deploy-rpi:
	@test -n "$(HOST)" || (echo "Usage: make deploy-rpi HOST=192.168.1.100" && exit 1)
	rsync -avz --exclude .git --exclude __pycache__ --exclude .venv \
		. pi@$(HOST):/opt/microgrid-agent/
	ssh pi@$(HOST) "sudo systemctl restart microgrid-agent"

docker-build:
	docker build -t microgrid-agent -f deploy/Dockerfile .

docker-run:
	docker run --rm -it microgrid-agent

health:
	./scripts/health-check.sh

install:
	pip install -e ".[dev,ingest]"
