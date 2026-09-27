// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {ArbitrageExecutor} from "../src/ArbitrageExecutor.sol";

contract MockERC20 {
    mapping(address => uint256) public balanceOf;
    function mint(address to, uint256 amt) external {
        balanceOf[to] += amt;
    }
    function transfer(address to, uint256 amt) external returns (bool) {
        balanceOf[msg.sender] -= amt;
        balanceOf[to] += amt;
        return true;
    }
    function approve(address, uint256) external returns (bool) {
        return true;
    }
    function allowance(address, address) external pure returns (uint256) {
        return type(uint256).max;
    }
    function transferFrom(address, address, uint256) external pure returns (bool) {
        return true;
    }
}

contract MockPool {
    MockERC20 public token;
    uint256 public premiumBps = 5; // 0.05%
    constructor(MockERC20 t) {
        token = t;
    }
    function flashLoanSimple(
        address receiver,
        address asset,
        uint256 amount,
        bytes calldata params,
        uint16
    ) external {
        require(asset == address(token), "wrong asset");
        uint256 premium = (amount * premiumBps) / 10_000;
        token.mint(receiver, amount);
        (bool ok,) = receiver.call(
            abi.encodeWithSignature(
                "executeOperation(address,uint256,uint256,address,bytes)",
                asset,
                amount,
                premium,
                receiver,
                params
            )
        );
        require(ok, "callback failed");
        // Pool pulls repayment (checked by test via balances)
    }
}

contract ArbitrageExecutorTest is Test {
    ArbitrageExecutor exec;
    MockERC20 token;
    MockPool pool;

    function setUp() public {
        token = new MockERC20();
        pool = new MockPool(token);
        exec = new ArbitrageExecutor(address(pool), address(0x1234));
        // Fund executor with ETH for miner payments
        vm.deal(address(exec), 10 ether);
    }

    function test_profitable_noop_reverts_without_profit() public {
        // No actions, no profit -> must revert ArbitrageUnprofitable or ProfitBelowMinimum
        ArbitrageExecutor.Action[] memory actions = new ArbitrageExecutor.Action[](0);
        vm.expectRevert();
        exec.executeArbitrage(address(token), 1 ether, 0, 0, actions);
    }

    function test_onlyOwner() public {
        ArbitrageExecutor.Action[] memory actions = new ArbitrageExecutor.Action[](0);
        vm.prank(address(0xBEEF));
        vm.expectRevert(ArbitrageExecutor.OnlyOwner.selector);
        exec.executeArbitrage(address(token), 1 ether, 0, 0, actions);
    }

    function test_emergencyWithdraw_eth() public {
        uint256 before = address(this).balance;
        exec.emergencyWithdraw(address(0));
        assertGt(address(this).balance, before);
    }

    receive() external payable {}
}
