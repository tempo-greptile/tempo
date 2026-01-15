# DEX Minimum Order Enforcement on Partial Fills

This document specifies a protocol change to prevent DoS attacks on the Stablecoin DEX by enforcing minimum order size after partial fills.

- **Spec ID**: TIP-DEX-MIN-ORDER
- **Authors/Owners**: @georgios, @dan
- **Status**: Draft
- **Related Specs**: Stablecoin DEX Specification

---

# Overview

## Abstract

When a partial fill on the Stablecoin DEX leaves an order with remaining amount below `MIN_ORDER_AMOUNT` ($100), the order is automatically completed:

- **Non-flip orders**: Cancelled with remaining tokens refunded to maker
- **Flip orders**: Force-completed with a proportional flip (flip amount = filled amount)

This prevents DoS attacks where malicious users create arbitrarily small orders by self-matching, while preserving flip order functionality.

## Motivation

### Problem

The current DEX enforces a $100 minimum order size at placement time, but not after partial fills. This creates a vulnerability:

1. User places a $100+ order (e.g., $150)
2. User trades against their own order to partially fill it (e.g., buy $60)
3. Order now has $90 remaining, below the minimum
4. Repeat to create arbitrarily small orders (e.g., $0.000001)

By stacking many tiny orders on the orderbook, an attacker can:
- Increase gas costs for legitimate swaps (more orders to iterate)
- Bloat storage with dust orders
- Degrade orderbook performance

### Solution

Extend the minimum order size enforcement to partial fills:

1. **Non-flip orders**: Auto-cancel when `remaining < MIN_ORDER_AMOUNT`, refunding remaining tokens to maker
2. **Flip orders**: Force-complete with proportional flip - the flip order amount equals the filled amount (not the original amount), preserving the flip functionality while preventing tiny orders

---

# Specification

## Constants

No new constants. Uses existing:

```solidity
uint128 public constant MIN_ORDER_AMOUNT = 100_000_000; // $100 with 6 decimals
```

## Key Insight: Proportional Flipping

For flip orders, we can compute the filled amount without additional storage:

```
filled = order.amount - order.remaining
```

When a flip order is force-completed, the flip order is created with `amount = filled`, not the original amount. This ensures:
- The flip is proportional to actual execution
- No new storage fields needed
- Fair treatment of flip order users

## Behavior Change

### Current Behavior

`partial_fill_order` updates `order.remaining` and leaves the order active regardless of the new remaining amount.

### New Behavior

After computing `new_remaining = order.remaining - fill_amount`:

1. If `new_remaining >= MIN_ORDER_AMOUNT` OR `new_remaining == 0`:
   - Continue with normal partial fill logic (no change)

2. If `0 < new_remaining < MIN_ORDER_AMOUNT` AND **order is NOT a flip order**:
   - Credit the filled amount to maker (normal settlement)
   - Refund remaining tokens to maker's internal balance
   - Remove order from orderbook, delete from storage
   - Emit `OrderFilled` + `OrderCancelled`

3. If `0 < new_remaining < MIN_ORDER_AMOUNT` AND **order IS a flip order**:
   - Credit the filled amount to maker (normal settlement)
   - Refund remaining tokens to maker's internal balance
   - Calculate `total_filled = order.amount - new_remaining`
   - Remove order from orderbook, delete from storage
   - If `total_filled >= MIN_ORDER_AMOUNT`:
     - Create flip order with `amount = total_filled` (proportional flip)
     - Flip order placed at `flip_tick`, with `is_flip = true`
   - If `total_filled < MIN_ORDER_AMOUNT`:
     - No flip order created (filled amount too small)
   - Emit `OrderFilled` + `OrderCancelled` (+ `OrderPlaced` if flip created)

## Interface Changes

No interface changes. Existing events are reused:

```solidity
event OrderFilled(
    uint128 indexed orderId,
    address indexed maker,
    address indexed taker,
    uint128 fillAmount,
    bool partialFill
);

event OrderCancelled(uint128 indexed orderId);

event OrderPlaced(
    uint128 indexed orderId,
    address indexed maker,
    address indexed base,
    uint128 amount,
    bool isBid,
    int16 tick,
    bool isFlip
);
```

## Affected Functions

- `partial_fill_order` (internal) - Primary change location
- `fill_orders_exact_in` - Calls `partial_fill_order`
- `fill_orders_exact_out` - Calls `partial_fill_order`

## Pseudocode

```rust
fn partial_fill_order(&mut self, order: &mut Order, level: &mut TickLevel, fill_amount: u128, taker: Address) -> Result<u128> {
    let new_remaining = order.remaining() - fill_amount;
    
    // Normal maker settlement for filled portion
    settle_maker(order, fill_amount);
    
    if new_remaining > 0 && new_remaining < MIN_ORDER_AMOUNT {
        // Refund remaining to maker
        refund_remaining_to_maker(order, new_remaining);
        
        // Remove from orderbook
        remove_from_linked_list(order, level);
        update_tick_level_liquidity(level, order.remaining());
        
        if level.head == 0 {
            clear_tick_bitmap(order);
            update_best_tick_if_needed(order);
        }
        
        delete_order(order);
        
        // Handle flip orders: create proportional flip if filled amount >= minimum
        if order.is_flip() {
            let total_filled = order.amount() - new_remaining;
            if total_filled >= MIN_ORDER_AMOUNT {
                // Create flip order with proportional amount
                place_flip(
                    order.maker(),
                    orderbook.base,
                    total_filled,        // Proportional flip amount
                    !order.is_bid(),     // Flip side
                    order.flip_tick(),   // New tick
                    order.tick(),        // New flip_tick
                    true,                // is_flip
                );
            }
            // If total_filled < MIN_ORDER_AMOUNT, no flip (too small)
        }
        
        emit_order_filled(order, fill_amount, partial_fill: true);
        emit_order_cancelled(order);
    } else {
        // Normal partial fill
        order.remaining = new_remaining;
        update_tick_level_liquidity(level, fill_amount);
        emit_order_filled(order, fill_amount, partial_fill: true);
    }
    
    Ok(amount_out)
}
```

---

# Invariants

1. **No orders below minimum**: After any swap, no active order has `0 < remaining < MIN_ORDER_AMOUNT`

2. **Maker made whole**: When force-completed, maker receives:
   - Settlement for filled portion (normal)
   - Full refund of remaining escrowed tokens
   - Proportional flip order (for flip orders with sufficient filled amount)

3. **Proportional flip**: Flip orders create flips with `amount = total_filled`, not original amount

4. **Accounting consistency**: Total liquidity at tick level equals sum of remaining amounts of all orders at that tick

5. **Event ordering**: `OrderFilled` → `OrderCancelled` → `OrderPlaced` (if flip created)

## Test Cases

### Non-Flip Orders
1. **Auto-cancel triggers**: Place $150 order, swap $60 → order cancelled, $90 refunded
2. **Boundary - at minimum**: Place $200 order, swap $100 → order remains with $100
3. **Boundary - just below**: Place $199 order, swap $100 → order cancelled, $99 refunded
4. **Full fill unaffected**: Place $100 order, swap $100 → normal full fill

### Flip Orders
5. **Flip with sufficient filled**: Place $200 flip order, swap $110 → cancelled, $90 refunded, $110 flip created
6. **Flip with insufficient filled**: Place $150 flip order, swap $60 → cancelled, $90 refunded, NO flip (filled < $100)
7. **Flip at exact minimum filled**: Place $190 flip order, swap $100 → cancelled, $90 refunded, $100 flip created
8. **Flip chain termination**: Flip order → force-complete → proportional flip → force-complete → no flip (amounts shrink naturally)

### Edge Cases
9. **Bid order refund**: Verify quote tokens refunded with correct rounding
10. **Ask order refund**: Verify base tokens refunded exactly
11. **Linked list integrity**: Multiple orders at tick, middle order force-completed
12. **Best tick updates**: Force-complete last order at best tick

---

# Examples

## Example 1: Non-Flip Order Auto-Cancel

```
1. Alice places $150 ask order at tick 0
2. Bob swaps $60 quote for base (fills $60 of Alice's order)
3. Remaining = $90 < $100 minimum
4. Order auto-cancelled:
   - Alice receives $60 base settlement (from fill)
   - Alice receives $90 base refund (remaining)
   - OrderFilled + OrderCancelled emitted
```

## Example 2: Flip Order with Proportional Flip

```
1. Alice places $200 flip ask at tick 10, flip_tick 0
2. Bob swaps $110 quote for base (fills $110)
3. Remaining = $90 < $100 minimum
4. Order force-completed:
   - Alice receives $110 quote settlement
   - Alice receives $90 base refund
   - Flip order created: $110 bid at tick 0 (flip_tick 10)
   - OrderFilled + OrderCancelled + OrderPlaced emitted
```

## Example 3: Flip Order with Insufficient Filled Amount

```
1. Alice places $150 flip ask at tick 10, flip_tick 0
2. Bob swaps $60 quote for base (fills $60)
3. Remaining = $90 < $100 minimum
4. Order force-completed:
   - Alice receives $60 quote settlement
   - Alice receives $90 base refund
   - NO flip order created ($60 < $100 minimum)
   - OrderFilled + OrderCancelled emitted
```

---

# Migration

This change requires a **hard fork** as it modifies consensus-critical behavior:

- Existing orders below minimum (if any exist from edge cases) will be force-completed on next interaction
- No state migration needed - change is forward-only
- Clients should handle:
  - Receiving `OrderCancelled` events for orders they didn't explicitly cancel
  - Flip orders completing with proportional (not original) amounts
