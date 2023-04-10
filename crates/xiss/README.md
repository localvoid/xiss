# XISS

Experimental compiler for modular CSS written in Rust.

## Why?

### [String Interning](https://en.wikipedia.org/wiki/String_interning)

Instead of relying on bundler heuristics for inlining class names, xiss is generating TypeScript files with const enums. When TypeScript is compiled with `tsc`, all const enum strings will be inlined and minifier will be able to perform const evaluation and convert dynamic class name strings into interned strings.

### Decoupling CSS compilation from JavaScript stack

It seems that all modern toolchains are going towards a full-stack JavaScript direction. xiss decouples CSS compilation from JavaScript stack and is designed for projects that use differents stacks for dynamic Server-Side Rendering.

Features like Code-Splitting is still the responsibility of a frontend toolchain stack. xiss just precompiles id maps, so that Server-Side stack will be able to use scoped minified ids.

## Features

### Module Scopes

### ID Minification

### ID Types

- Class Names
- Vars
- Keyframes

### Class Maps

#### Format

- Inline conditional expression
- Table
- Auto

#### Declaring states

```css
@classmap buttonClass {
  disabled: ButtonDisabled;
  focus: ButtonFocus Focus;
}
```

#### Declaring static class names with `@static`

```css
@classmap buttonClass {
  @static Button;

  disabled: ButtonDisabled;
}
```

#### Excluding states with exclude constraints `@exclude`

```css
@classmap buttonClass {
  @static Button;

  disabled: ButtonDisabled;
  focus: ButtonFocus;

  @exclude disabled focus;
}
```

### External IDs

```css
@extern class Button from 'xiss/buttons';
@extern class Button as myButton from 'xiss/buttons';
```

### Constants

```css
:const {
  --MAIN-BACKGROUND: #333;
}
```

## Exclude filters

## CSS Map

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
