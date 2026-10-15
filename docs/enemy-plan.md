# Plan: stronger, more varied enemies

The steps go from easiest to hardest. Each one works on its own, so you can stop after any of them and still have a working game.

## Where things live now

| What | Where |
|---|---|
| Monster kinds and their stats | `EnemyKind`, `EnemyStats`, `stats()` in `src/game/play/combat.rs` |
| Which kind spawns (weighted, deeper = tougher) | `EnemyKind::pick` in `combat.rs` |
| How many spawn (one roll per room) | `place_items_and_monsters` in `src/game/levels/populate.rs:61` |
| What enemies do each turn | `enemies_act`, `enemy_attacks`, `step_toward_player` in `combat.rs` |
| A monster's picture on the map | `enemy_tile` in `src/game/ui/minimap.rs` |
| A monster's sprite in 3D | the `icons` numbers (Raven pack) in `stats()` |

Right now every monster does the same thing: walk at you and hit for a fixed amount. They only differ in HP, damage, speed, and how often they act.

---

## Step 1: add a new monster (easy, data only)

You learn how a kind goes from its stats to the map and then to the 3D view.

1. Add a variant to `EnemyKind`, for example `Skeleton`.
2. Add it to `EnemyKind::ALL` and raise the number in `[EnemyKind; 5]`.
3. Add a match arm in `stats()`: name, Raven icon number(s), hp, damage, and weights.
   Give it `weight: 0, weight_per_depth: 2` so it only shows up deeper down. That's the Shadow Blob's trick.
4. Run `cargo build`. The compiler will complain that `enemy_tile` in `minimap.rs` doesn't cover the new kind. Give it a picture, for example `glyph::SKULL`.

Ideas with pictures already on the sheet: Skeleton (`SKULL`), Ghost (`GHOST`), Goblin (`GOBLIN`), Wizard (`WIZARD`), Eye (`EYE`).

**Check:** in the debug build, turn fog off (F3) and use the level generator to go deep. The new monster should show up on the map.

---

## Step 2: tougher monsters deeper down (easy)

Right now a Grey Rat on depth 8 is as weak as one on depth 1.

- In `Enemy::new` (combat.rs), pass in `depth` and give the monster `s.hp + depth as i32 / 2` HP.
- Add a `damage` field to `Enemy` that does the same thing, and use it in `enemy_attacks` instead of `stats.damage`.
  Hint: `enemy_attacks` takes `&EnemyStats`; change it to take the enemy's index `i` instead.
- `populate.rs:84` calls `Enemy::new`, and it already has `depth` in scope.

**Check:** `cargo test`. Fix any test that calls `Enemy::new` by adding a depth argument (`tests.rs` has one).

---

## Step 3: more monsters per room deeper down (easy)

In `place_items_and_monsters`, each room rolls for **one** monster. Change that to a small loop: roll `1 + depth / 3` times. Stop when `free_tile` returns `None`, which means the room is full.

Watch out: if too many monsters start combat at once, the game gets hard fast. Cap it, for example at `.min(4)`.

---

## Step 4: a "behavior" for each kind (medium, the big one)

This is where monsters start to feel different. Add a field to `EnemyStats`:

```rust
pub(crate) enum Behavior {
	/// Walks up and hits (what everyone does now)
	Melee,
	/// Stays at range and hits from up to N tiles away in a straight line
	Ranged(i32),
	/// Runs away when low on HP
	Coward,
	/// Never moves; hits anything next to it (the Thorn Vine is this already, via steps: 0)
	Stationary,
}
```

Then in `enemies_act`, `match stats.behavior { ... }` instead of always doing "attack if adjacent, else step closer".

Suggested order:
1. **Ranged** (a Wizard). If the player is in a straight line (same `x` or same `y`) and within N tiles, attack. Otherwise step closer. You'll need a "clear line" check that walks the tiles between and stops at walls (`self.map.is_wall`).
2. **Coward** (a Goblin). If `hp <= max/3`, step *away*. Copy `step_toward_player` and flip the signs.

**Check:** add a test in `src/game/play/tests.rs`, copying the setup of `combat_snaps_to_tiles_and_bumping_an_enemy_attacks_it`. Put a ranged enemy 3 tiles away in a straight line, call `world.enemies_act()`, and check that `world.hero.hp` went down.

---

## Step 5: special attacks (medium)

Add an optional effect to `EnemyStats`, for example `on_hit: Option<Effect>`:

- **Poison** (Green Spider): the hero loses 1 HP per turn for 3 turns. This needs a `poison: u32` counter on the hero (`world.hero`, in items.rs) that ticks down at the start of the enemies' turn.
- **Drain** (Shadow Blob): heals itself by the damage dealt.
- **Split** (Shadow Blob): when it dies, spawn two smaller ones. Do this in `strike`, where it calls `self.enemies.remove(i)`. Push new `Enemy`s at free tiles next to it.

Tell the player what happened with `self.say(...)` each time, so they can see why.

---

## Step 6: a boss on the deepest levels (harder)

- A new kind with `weight: 0, weight_per_depth: 0`, so `pick` never picks it.
- In `populate.rs`, every 5th depth (`depth % 5 == 4`), place it in the last room on purpose.
- Give it a lot of HP, the `Ranged` behavior, and maybe make it immune to the first hit.
- Bonus: drop guaranteed loot when it dies. Look at how chests do it in `src/game/play/chests.rs`.

---

## Things to remember every time

- **Picking icons:** the `icons` numbers come from the Raven pack. You can list several and one is picked per monster.
- **Balance:** the player has 10 HP (`PLAYER_HP`) and hits for 2 (`PLAYER_DAMAGE`, +`SWORD_BONUS`). A monster that does 3 damage is scary.
- **The compiler is your checklist:** after adding a kind or a field, `cargo build` points at every place that needs updating.
- **Testing:** `src/game/play/tests.rs` builds worlds from fixed seeds. Copy an existing test as a starting point.
