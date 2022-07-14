#!/usr/bin/env bash

# Run the benchmark using 
# t3 - three threads
# c100 - one hundred concurrent connections
# d30s - 30 seconds
# R2000 - 2000 requests per second (total, across all connections combined)

wrk2 -s ./sentry/benchmark/benchmark.lua -t3 -c100 -d30s -R2000 --latency \
http://127.0.0.1:8005/v5/campaign/0x936da01f9abd4d9d80c702af85c822a8/events