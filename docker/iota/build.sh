#!/usr/bin/env bash
# Copyright (c) Mysten Labs, Inc.
# Modifications Copyright (c) 2024 IOTA Stiftung
# SPDX-License-Identifier: Apache-2.0
export CARGO_BUILD_FEATURES=indexer
./../utils/build-script.sh --image-tag "angelovtt/iota:1.4.0-alpha"
