// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Script, console2} from "forge-std/Script.sol";
import {ArbitrageExecutor} from "../src/ArbitrageExecutor.sol";

contract Deploy is Script {
    function run() public {
        address pool = vm.envAddress("AAVE_V3_POOL");
        address provider = vm.envAddress("AAVE_V3_PROVIDER");
        vm.startBroadcast();
        ArbitrageExecutor exec = new ArbitrageExecutor(pool, provider);
        vm.stopBroadcast();
        console2.log("ArbitrageExecutor deployed at:", address(exec));
    }
}
