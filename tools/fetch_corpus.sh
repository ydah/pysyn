#!/bin/sh
set -eu

mkdir -p corpus vendor
git clone --depth 1 --branch 3.13 https://github.com/python/cpython vendor/cpython
cp -R vendor/cpython/Lib corpus/stdlib
for repository in django/django psf/requests numpy/numpy pandas-dev/pandas; do
    name=${repository##*/}
    git clone --depth 1 "https://github.com/${repository}" "vendor/${name}"
done
