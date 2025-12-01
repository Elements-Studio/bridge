// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Note: These E2E tests require a complete TestCluster implementation with Starcoin nodes.
// The current test-cluster crate is a stub. To run full E2E tests:
// 1. Start the local environment: ./setup.sh -y --without-bridge-server
// 2. Run local_env_tests which test against the running environment
//
// The basic.rs and complex.rs tests below are temporarily disabled until
// a complete TestCluster implementation is available.

// #[cfg(test)]
// mod basic;
// #[cfg(test)]
// mod complex;

// pub mod test_utils;

#[cfg(test)]
mod local_env_tests;
