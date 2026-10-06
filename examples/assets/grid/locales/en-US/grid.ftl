grid-title = Grid layout
grid-hint = Arrows, D-pad or left stick: move through the grid · <kbd>Enter</kbd> / A: press · <kbd>Q</kbd> <kbd>E</kbd> / LB RB: tabs · <kbd>Space</kbd> / X: layout · <kbd>L</kbd> / Y: language; resize the window to reflow the slots.
grid-layout = Layout
grid-language = Language: English
grid-tab-items = Items
grid-tab-gear = Gear
grid-tab-quests = Quests
grid-details = Details
grid-stat-items = Item kinds
grid-stat-weight = Weight
grid-stat-gold = Gold
grid-status =
    { $layout ->
        [wide] Wide layout: three columns, banner and status bar span them all.
       *[narrow] Narrow layout: one column, the panels stack in source order.
    }
grid-item-map = Map
grid-item-sword = Sword
grid-item-shield = Shield
grid-item-potion = Potion
grid-item-herb = Herb
grid-item-lantern = Lantern
grid-item-rope = Rope
grid-item-gem = Gem
grid-item-key = Key
grid-screen =
    { $tab ->
        [gear] Gear: a fixed 3 × 4 grid, every slot placed by line numbers, the figure spanning two rows.
        [quests] Quests: three equal columns of nested card grids under a main quest spanning all three.
       *[items] Items: as many columns as fit (auto-fill), a 2 × 2 featured slot, dense packing.
    }
grid-gear-hero = Ada
grid-gear-figure = the Brave
grid-gear-neck = Neck
grid-gear-head = Head
grid-gear-back = Back
grid-gear-main-hand = Main hand
grid-gear-off-hand = Off hand
grid-gear-ring = Ring
grid-gear-trinket = Trinket
grid-gear-feet = Feet
grid-gear-empty = — empty —
grid-item-helm = Helm
grid-item-amulet = Amulet
grid-item-signet = Signet ring
grid-item-boots = Boots
grid-quests-active = Active
grid-quests-done = Done
grid-quests-failed = Failed
grid-quest-descend = Reach the lowest vault
grid-quest-torches = Ration the torches
grid-quest-map = Fix the map
grid-quest-key = Find the lost key
grid-quest-rope = Recover the rope
grid-quest-rats = Clear the cistern
grid-quest-bridge = Cross the eastern chasm
grid-quest-reward = { $gold } gold
grid-quest-progress =
    { $status ->
        [done] Completed
        [failed] Failed at { $progress } %
       *[other] { $progress } % done
    }
