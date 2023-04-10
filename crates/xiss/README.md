# XISS

Experimental compiler for modular CSS written in Rust.

## Supported ID types

- Class names
- Vars
- Keyframes

## Global IDs

- Minification
- Performance

## Class Maps

- Inline
- Table
- Auto

### Declaring states

```css
@classmap buttonClass {
  disabled: ButtonDisabled;
  focus: ButtonFocus Focus;
}
```

### Declaring static class names with `@static`

```css
@classmap buttonClass {
  @static Button;

  disabled: ButtonDisabled;
}
```

### Excluding states with exclude constraints `@exclude`

```css
@classmap buttonClass {
  @static Button;

  disabled: ButtonDisabled;
  focus: ButtonFocus;

  @exclude disabled focus;
}
```

## External IDs

```css
@extern class Button from 'xiss/buttons';
@extern class Button as myButton from 'xiss/buttons';
```

## Constants

```css
:const {
  --MAIN-BACKGROUND: #333;
}
```

## Exclude filters

## CSS Map Files

### Format

CSS Map files are stored in a CSV format with four columns:

- ID kind
  - `C` - Class name
  - `V` - Var
  - `K` - Keyframes
- Module ID
- Local ID
- Global ID

E.g.

```csv
C,xiss/example,Button,a
V,xiss/example,MyVar,a
K,xiss/example,anim,a
C,xiss/test,Slider,b
C,xiss/test,SliderDisabled,c
C,xiss/test,SliderActive,d
```

### Lock File
#### Defining static IDs
#### Reserve short IDs for frequently used IDs
