#!/bin/sh

if [ -z "$1" ]; then
  echo "Usage: $0 <INTERVAL_US> [EXTRA FLASH OPTIONS]"
  exit 1
fi
INTERVAL=$1
shift

export INTERVAL

DOCKER_OPTS="-e INTERVAL=${INTERVAL}" robo flash "$@"
