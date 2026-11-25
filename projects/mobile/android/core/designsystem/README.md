# core/designsystem

The Home app's design system: tokens, component styles and component views.
It depends only on `core/theme` (colours, `colorStateOn`, `colorWarning`,
`colorSuccess`, `homeFab*`); every screen module depends on it. Colours are
always theme attributes — this module defines no hex values, so every
component looks right in Emerald, System and Neon, light and dark.

`Base.Theme.Home` wires the defaults: FAB and extended FAB styles,
`textInputStyle`, `HomeViewInflater` (every `TextInputLayout` becomes
`CardTextInputLayout`), popup menus, `materialSwitchStyle`, shape appearances
and `homeDeviceTileStyle`. The app's `Theme.Home` inherits it and adds only
the colour roles.

## Tokens

| Token | Values |
| --- | --- |
| Spacing `ds_space_*` | xs 4, s 8, m 12, l 16, xl 24, xxl 32 dp |
| Radius `ds_radius_*` | s 8 (chips), m 12 (S buttons, FAB label, small shapes), l 16 (buttons, fields, medium shapes), xl 28 (cards, popups, sheets), full |
| Control height `ds_control_*` | s 32, m 40, l 48 dp; touch target `ds_touch_target` 48 dp |
| Icons `ds_icon_*` | s 18, m 24 dp |
| Stroke `ds_stroke_*` | thin 1, thick 2 dp |
| Typography | Material 3 roles through the theme (`?attr/textAppearanceTitleMedium`, `…BodyLarge`, `…LabelLarge`, …) plus `TextAppearance.Home.{Time,Countdown,ToolbarTitle,Pill,PillTab,ChatMeta,PopupMenuItem}` |
| Popup `homePopup*` | surface `colorSurfaceContainerHighest` (lighter than card surfaces in dark themes, darker in light), stroke `colorPrimary` at 50% alpha, stroke width 1 dp, radius `ds_radius_xl` 28 dp, elevation `ds_popup_elevation` 8 dp; item text `colorOnSurface`, item icons `colorPrimary`, pressed/hover ripple `colorPrimary` at 20%/8% |
| Row ripple `homeRowItemRippleColor` | `colorPrimary` at 20% pressed, 8% hovered; `addGroupedRows` clips it to each row's card shape |
| Motion | `HomeMotion` (enter/exit/selection durations and easings from the theme) |

## Components

| Component | How to use | Notes |
| --- | --- | --- |
| Button | `MaterialButton` + `style="@style/Widget.Home.Button.<Variant>.<Size>"` | Variants `Filled`, `Tonal`, `Outlined`, `Text`, `Destructive`; sizes `S` 32, `M` 40, `L` 48 dp. One line, no insets. M/L (not Text) are at least 120 dp wide. Disabled: `android:enabled=false`. Loading: `button.setLoading(true)` replaces the text with a progress indicator and keeps the width. |
| Button row | `ButtonRow` with the secondary action first and the main one last | Side by side (main on the right) when both fit at their natural width; otherwise stacked full width, main on top. Button text never wraps or gets cut (`maxLines=1` in every button style). `app:equalWidth` gives every button the widest one's width, `app:centered` centres the group (empty states). At most one Filled button per row (static check). |
| Special buttons | `Widget.Home.Button.Pill.Outlined` (outlined 32dp pill), `.DayToggle` (checkable weekday circle), `.BarAction` (icon over label in a bottom bar), `.TimeDisplay` (large time in editors) | Use only for these roles. |
| Icon button | `MaterialButton` + `Widget.Home.IconButton.<Standard\|Filled\|Tonal\|Outlined>.<S\|M\|L>` + `app:icon` + `contentDescription` | Visible 32/40/48 dp circle, always a 48 dp touch target (insets). |
| FAB | `OutlinedFloatingActionButton` (theme default), `Widget.Home.FloatingActionButton.Small`, `ExtendedFloatingActionButton` (theme default), quick action `Widget.Home.FloatingActionButton.SmallSecondary` + label `Widget.Home.FabLabel` | Outline and glow come from `homeFab*` in every theme. |
| Toolbar title | `ToolbarTitle` | Picks the largest size from 24sp down to 16sp at which the title fits in two lines without breaking a word (balanced breaks, no hyphenation, no ellipsis). AppCompat autoSize is not used: it does nothing with `wrap_content` width, which is what the toolbar gives its title. |
| Field supporting text | `TextView` + `Widget.Home.FieldSupportingText` (`.Error`) | Status lines under a field: same 16dp indent as the field's helper, 8dp above. |
| Text field | `TextInputLayout` without `style` (theme default `Widget.Home.TextInput`, card-like) | Chat message field: `Widget.Home.TextInput.Chat`. |
| Card | `MaterialCardView` + `Widget.Home.Card` / `.Selected` / `.StateOn` | Filled `colorSurfaceContainer`, radius xl. |
| Chip | `Chip` + `Widget.Home.Chip.Filter` / `.Assist` / `.Input` | From code: `Components.filterChip(parent)`, `Components.inputChip(parent)`. |
| Switch | `MaterialSwitch` (theme default `Widget.Home.Switch` from core/theme) | Checked track `colorStateOn`. |
| Slider | `Slider` without `style` (theme default `Widget.Home.Slider`) | Active track `colorPrimary`, inactive `colorSurfaceContainerHighest`, active ticks `colorOnPrimary`, inactive ticks `colorOnSurfaceVariant`, thumb `colorPrimary`. Gradient sliders: `GradientSlider` + `Widget.Home.Slider.Gradient`; the gradient is the track and ticks are hidden. |
| Pills | `PillSegments` | The only choice-between-options control; no segmented buttons. |
| Popup menu | `HomePopupMenu(anchor, menuRes)` | Card with 48 dp rows and icons. Every popup in the app goes through it: toolbar overflow, chat attach and message actions, access token actions; context-style menus are the same card. `popupMenuStyle`, `actionOverflowMenuStyle`, `android:contextPopupMenuStyle` and `listPopupWindowStyle` point at the same background and elevation, so any native `PopupMenu`, `ListPopupWindow` or exposed dropdown (`AutoCompleteTextView`) inherits it. The card surface is `homePopupSurface` (`colorSurfaceContainerHighest`), the 1 dp `homePopupStroke` is the accent at 50% alpha, icons use `homePopupItemIconTint` (`colorPrimary`), labels `homePopupItemTextColor` (`colorOnSurface`), and the ripple `homePopupItemRippleColor` shows pressed and hover states. Opens below the anchor when it fits, otherwise above; stays inside the window insets (system bars, IME, cutout) and never covers a view marked with `HomePopupMenu.markObstacle` (the bottom navigation); scrolls inside when neither side fits. |
| List row | `ListRow` (`app:rowIcon`, `rowTitle`, `rowSubtitle`, `rowValue`, `rowChevron`, `rowSwitch`); child views in XML become the trailing slot | Min height 56 dp; `subtitleTone` = `NEUTRAL`/`ERROR`/`WARNING`/`SUCCESS`; `app:rowIconTint` (default `colorOnSurfaceVariant`, `colorPrimary` for accent icons such as permissions); titles use a 20sp line height so a wrapped title stays compact. |
| Section header | `TextView` + `Widget.Home.SectionHeader` | TitleSmall in `colorPrimary`. |
| Empty state | `EmptyState` (`app:emptyIcon`, `emptyText`, `emptyAction`) | Main action Filled M (`action`, `app:emptyAction`), optional secondary Tonal M (`secondaryAction`, `app:emptySecondaryAction`) in a centred equal-width `ButtonRow`. |
| Status chip | `TextView` + `Widget.Home.StatusChip.<Error\|Warning\|Success\|Neutral>` | 24 dp pill. |
| Device tile | `DeviceTile` (`app:tileIcon`, `tileTitle`, `tileStatus`, `tileOn`) | On/off via `DeviceStateViews`. |

## Which variant and size

- The action the screen exists for: Filled **L**. In an editor's bottom save
  bar: Filled **L**, full width.
- Actions inside a card: Tonal **M** (Filled **M** for the card's main action),
  as wide as the text (min 120 dp), aligned to the card's content start; two
  actions sit in a row with the main one on the right.
- Removing or clearing something: Destructive **M** (outlined, `colorError`).
- Links and secondary actions: Text (size of the surrounding controls).
- Small inline actions in dense rows: **S**.
- Icon-only actions: Icon button **M** (S in dense rows, L for prominent ones).
- Two actions of one card: `ButtonRow`, never a fixed horizontal row.
- Empty and error states: one Filled main action (Open Connection), the other
  Tonal (Retry), equal width, centred, side by side when they fit. A lone
  Retry stays Tonal where it shares the row with Open Connection.
- A permission or system-setting row: `ListRow` with the icon, the title and
  the state as subtitle (`Tone.ERROR` while not allowed, neutral when
  allowed) and a trailing Tonal Icon button **M** with `ds_ic_open_in_new`
  («Открыть настройки: …» as content description). The whole row and the
  button open the same settings screen in both states, so the row never
  changes shape.
- Buttons built in code: `Components.tonalButtonL(parent)`; no
  `MaterialButton(context)`.

## Static check

`platform/quality/test_static.py`, class `AndroidDesignSystemTests`, runs in
CI with the other static tests:

- outside `core/designsystem` (and `core/theme`, which owns
  `Widget.Home.Switch` for the theme overlays) no values file declares a
  `Widget.*` style;
- in every layout outside the module, `MaterialButton`/`Button` must have a
  `Widget.Home.Button.*` or `Widget.Home.IconButton.*` style;
  `TextInputLayout`, `FloatingActionButton`, `ExtendedFloatingActionButton`,
  `OutlinedFloatingActionButton` and `Slider` either have no style (the theme
  default is the design-system style) or a `Widget.Home.TextInput*`/
  `Widget.Home.(Extended)FloatingActionButton*`/`Widget.Home.Slider*` one;
  `MaterialCardView` must use `Widget.Home.Card*` and `Chip`
  `Widget.Home.Chip.*`;
- Kotlin outside the module never calls the `MaterialButton(`,
  `TextInputLayout(`, `FloatingActionButton(`, `ExtendedFloatingActionButton(`,
  `MaterialCardView(` or `Chip(` constructors — use a layout or `Components`.

## Catalog

Debug builds (`FLAG_DEBUGGABLE`) show **Design system** at the end of
Settings; it opens `DesignCatalog` (`layout/ds_catalog.xml`): every component
in every variant, size and state. `//projects/mobile/android/core/designsystem:designsystem_test`
renders it in every theme, light and dark, and writes one PNG per section
(`catalog-<theme>-<dark|light>-<section>.png`). On Robolectric the System
theme has no wallpaper colours, so it renders the Emerald palette.

Tests in the same target: every button variant is 32/40/48 dp in every theme;
Destructive text and stroke are `colorError`; loading keeps the width; icon
buttons keep a 48 dp touch target; every FAB has the same outline width and
the theme's outline colour.

## Adding a component

1. Style or view in this module, colours only from theme attributes, sizes
   from the tokens; resource names start with `ds_`, styles with
   `Widget.Home.`.
2. Add it to `ds_catalog.xml` in every variant and state (a new section also
   goes into `DesignCatalog.sections`).
3. A test for its contract in `src/test`, a row in the table above.

## Adding a theme

Themes live in `core/theme` (see the Android README, «Themes»). A theme must
define every attribute the components read: Material colour roles,
`colorStateOn`/`colorOnStateOn`/`colorStateOnText`, `colorWarning`,
`colorSuccess` (with on-colours) and `homeFab*`. The catalog test then renders
the new theme automatically.

## Заметки к коду

- `GradientSlider` рисует полосу до `super.onDraw` по `trackSidePadding`,
  `trackWidth` и `trackHeight` самого `Slider`, поэтому градиент совпадает с
  треком и thumb едет по нему; отдельных `View` под полосой нет. В RTL
  градиент разворачивается вместе с треком. В `Widget.Home.Slider.Gradient`
  трек прозрачный, а тики выключены: на оранжево-белом и радужном градиенте
  любой из ролейных цветов тиков теряет контраст. Полоса центрируется по
  высоте `Slider`, поэтому стиль рассчитан на `labelBehavior=floating`
  (по умолчанию).
- У `Widget.Home.Slider` неактивный трек — `colorSurfaceContainerHighest`, а
  не M3-ный `colorSecondaryContainer`: в Emerald и Neon secondary-контейнер
  почти совпадает с фоном шторки, и граница «активно/неактивно» пропадает.
- `ButtonLoading.kt` fixes `minWidth` to the current width and centres the
  indicator with `ICON_GRAVITY_START` plus a start padding of
  (width − icon size) / 2: `textStart` gravity with no text drifts left.
- Catalog screenshots replace the spinner with a static arc
  (`freezeLoadingFrame`, tests only): Robolectric never advances the
  indeterminate animation.
- `HomePopupMenu`: the popup background has an 8dp transparent inset, so the
  window touches the anchor (`ANCHOR_GAP` 0) and the card still sits 8dp away.
  `ds_bg_popup_menu` paints the inset card from `homePopupSurface` with a 1dp
  `homePopupStroke`; `PopupStylesTest` checks both tokens in every theme, dark
  and light.
- `addGroupedRows()` (`list`) is the one entry point for grouped lists: it adds
  `GroupCards`, `GroupDividers` and `GroupRowRipples`. `GroupRowRipples`
  resolves each row's `GroupSlot` in `getItemOffsets`, which RecyclerView calls
  after every bind and after every adapter change, so a row that moves from the
  middle to the edge of a group gets the new shape without a rebind. The
  per-view `GroupedRowRipple` (kept in the `ds_grouped_row_ripple` tag) is a
  `RippleDrawable` in `homeRowItemRippleColor` whose mask is the same
  `GroupSlot.shape(radius)` the card uses: rounded top for the first row,
  rounded bottom for the last, the plain row rectangle in between. Only
  clickable rows get the ripple, so the adapter sets `isClickable`; rows must
  not declare `selectableItemBackground` themselves.
- `GroupedRowsLayout` (`list`) is the same contract for a static card: a
  vertical `LinearLayout` placed inside a `MaterialCardView` that binds a
  `GroupedRowRipple` to every visible child on layout, using the card's corner
  radius. Non-clickable children (a summary line) keep the slot order but get
  no ripple.
- `ListRow` carries its own `ds_space_l` horizontal padding
  (`Widget.Home.ListRow`), so a row inside a grouped card needs no per-layout
  padding. A row inside a container that already pads its content uses
  `style="@style/Widget.Home.ListRow.Embedded"`.
- `Context.joinMeta(parts)` joins metadata fragments with `ds_meta_separator`;
  features do not hard-code the separator.
- `GroupedRowsScreenshotTest` renders first, middle and last pressed rows over
  the theme window background in dark and light. Robolectric does not paint the
  patterned API 35 ripple on a software canvas, so the test paints the row's
  real ripple mask (shape and bounds taken from its `foreground`) in the
  pressed colour and checks that corners outside the shape stay untouched.
