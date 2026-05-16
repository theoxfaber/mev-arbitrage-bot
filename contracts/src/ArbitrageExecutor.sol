// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import {IPool} from "@aave/core-v3/contracts/interfaces/IPool.sol";
import {IPoolAddressesProvider} from "@aave/core-v3/contracts/interfaces/IPoolAddressesProvider.sol";
import {IFlashLoanSimpleReceiver} from "@aave/core-v3/contracts/flashloan/interfaces/IFlashLoanSimpleReceiver.sol";
import {IERC20} from "@aave/core-v3/contracts/dependencies/openzeppelin/contracts/IERC20.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// @title ArbitrageExecutor
/// @author MEV Arbitrage Engine v3 (Rust)
/// @notice Atomic flash-loan-powered arbitrage executor.
///
/// Design:
///   - On-chain actions support per-action ERC-20 approval/revoke cycles
///     to prevent lingering allowances.
///   - `block.coinbase.call{}` for builder-contract-safe miner payments.
///   - Custom errors (no require strings) for gas efficiency.
///   - `gasUsed` tracking in events for off-chain PnL analysis.
contract ArbitrageExecutor is IFlashLoanSimpleReceiver, ReentrancyGuard {
    address public immutable owner;
    IPool public immutable POOL;
    IPoolAddressesProvider public immutable ADDRESSES_PROVIDER_CONTRACT;

    event ArbitrageExecuted(
        address indexed asset,
        uint256 loanAmount,
        uint256 profit,
        uint256 minerReward,
        uint256 gasUsed
    );
    event EmergencyWithdrawal(address indexed token, uint256 amount);

    error OnlyOwner();
    error UnauthorizedCallback();
    error UntrustedInitiator();
    error ActionCallFailed(uint256 index, bytes reason);
    error ArbitrageUnprofitable(uint256 balanceAfter, uint256 required);
    error ProfitBelowMinimum(uint256 profit, uint256 minProfit);
    error MinerPaymentFailed();
    error InsufficientETHForMiner(uint256 available, uint256 required);

    modifier onlyOwner() {
        if (msg.sender != owner) revert OnlyOwner();
        _;
    }

    struct Action {
        address target;
        uint256 value;
        bytes data;
        address approveToken;
        uint256 approveAmount;
    }

    constructor(address _pool, address _addressesProvider) {
        owner = msg.sender;
        POOL = IPool(_pool);
        ADDRESSES_PROVIDER_CONTRACT = IPoolAddressesProvider(_addressesProvider);
    }

    function executeArbitrage(
        address asset,
        uint256 amount,
        uint256 minProfit,
        uint256 minerReward,
        Action[] calldata actions
    ) external onlyOwner nonReentrant {
        if (minerReward > 0 && address(this).balance < minerReward) {
            revert InsufficientETHForMiner(address(this).balance, minerReward);
        }
        bytes memory params = abi.encode(actions, minerReward, minProfit);
        POOL.flashLoanSimple(address(this), asset, amount, params, 0);
    }

    function executeOperation(
        address asset,
        uint256 amount,
        uint256 premium,
        address initiator,
        bytes calldata params
    ) external override returns (bool) {
        uint256 gasStart = gasleft();

        if (msg.sender != address(POOL)) revert UnauthorizedCallback();
        if (initiator != address(this)) revert UntrustedInitiator();

        (Action[] memory actions, uint256 minerReward, uint256 minProfit) =
            abi.decode(params, (Action[], uint256, uint256));

        uint256 balanceBefore = IERC20(asset).balanceOf(address(this));

        uint256 len = actions.length;
        for (uint256 i = 0; i < len;) {
            Action memory action = actions[i];

            if (action.approveToken != address(0)) {
                uint256 amt = action.approveAmount == 0 ? type(uint256).max : action.approveAmount;
                IERC20(action.approveToken).approve(action.target, amt);
            }

            (bool success, bytes memory returnData) =
                action.target.call{value: action.value}(action.data);

            if (!success) {
                if (returnData.length > 0) {
                    assembly { revert(add(32, returnData), mload(returnData)) }
                }
                revert ActionCallFailed(i, returnData);
            }

            if (action.approveToken != address(0)) {
                IERC20(action.approveToken).approve(action.target, 0);
            }

            unchecked { ++i; }
        }

        uint256 amountOwed = amount + premium;
        uint256 balanceAfter = IERC20(asset).balanceOf(address(this));

        if (balanceAfter < balanceBefore + amountOwed)
            revert ArbitrageUnprofitable(balanceAfter, balanceBefore + amountOwed);

        uint256 profit = balanceAfter - (balanceBefore + amountOwed);
        if (profit < minProfit)
            revert ProfitBelowMinimum(profit, minProfit);

        if (minerReward > 0) {
            (bool paid, ) = block.coinbase.call{value: minerReward}("");
            if (!paid) revert MinerPaymentFailed();
        }

        IERC20(asset).approve(address(POOL), amountOwed);

        emit ArbitrageExecuted(asset, amount, profit, minerReward, gasStart - gasleft());
        return true;
    }

    function emergencyWithdraw(address token) external onlyOwner {
        uint256 amt;
        if (token == address(0)) {
            amt = address(this).balance;
            (bool ok, ) = owner.call{value: amt}("");
            if (!ok) revert MinerPaymentFailed();
        } else {
            amt = IERC20(token).balanceOf(address(this));
            IERC20(token).transfer(owner, amt);
        }
        emit EmergencyWithdrawal(token, amt);
    }

    function batchRevokeApprovals(
        address[] calldata tokens,
        address[] calldata spenders
    ) external onlyOwner {
        for (uint256 i = 0; i < tokens.length;) {
            IERC20(tokens[i]).approve(spenders[i], 0);
            unchecked { ++i; }
        }
    }

    function ADDRESSES_PROVIDER() external view override returns (IPoolAddressesProvider) {
        return ADDRESSES_PROVIDER_CONTRACT;
    }

    receive() external payable {}
}
