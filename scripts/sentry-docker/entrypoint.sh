#!/usr/bin/env bash

set -o errexit
set -o pipefail

>&2 echo "Waiting for redis..."
REDIS_HOST=${REDIS_HOST:-redis}
REDIS_PORT=${REDIS_PORT:-6379}
./scripts/wait-for-it.sh -h ${REDIS_HOST} -p ${REDIS_PORT} -t 10
>&2 echo "Redis is up - continuing..."

>&2 echo "Waiting for postgres..."
POSTGRES_HOST=${POSTGRES_HOST:-postgres}
POSTGRES_PORT=${POSTGRES_PORT:-5432}
./scripts/wait-for-it.sh -h ${POSTGRES_HOST} -p ${POSTGRES_PORT} -t 10
>&2 echo "Postgres is up - continuing..."

exec "$@"
