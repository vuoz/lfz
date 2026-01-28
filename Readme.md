### L(ocal) F(first) Z(MK builds)
[normal](https://github.com/user-attachments/assets/ddf8eb61-a803-49c2-a857-c325f53dcc4d)
#### Disclaimer 
This is highly experimental and largly coded by claude.   
Use at your own risk!  
I have tested it on all my configs that use a variety of features like external modules and custom board defintions.
There might still be edge cases that I have not covered.

#### Prerequisites
- Docker
- Rust 


### Install 

#### Locally 
1. Clone this repo
2. `cargo install --path .`

#### Or install from tags.


### Usage
This will build all targets found in `build.yml`/`build.yaml` and output them to `/zmk-target`
```bash
lfz
```


#### Incremental builds ( much faster ) ( might produce undefined build behavoir)

```bash
lfz -i
```
[incremental](https://github.com/user-attachments/assets/ed9f15a1-4844-4002-a87c-7090ef5e5b98)

### Groups
You can add a group to each target in the build.yml file.
This allows you to only build relevant targets. 
If you are just changing your keymap this allows you to iterate quickly by only building central.

```diff
---
include:

  - board: zongle
    shield: chalk_dongle
    snippet: studio-rpc-usb-uart 
    cmake-args: -DCONFIG_ZMK_STUDIO=y -DCONFIG_ZMK_SPLIT_ROLE_CENTRAL=y
++  group: central

  - board: xiao_ble 
    shield: chalk_left 
    cmake-args: -DCONFIG_ZMK_SPLIT=y -DCONFIG_ZMK_SPLIT_ROLE_CENTRAL=n
++  group: peripheral

  - board: xiao_ble 
    shield: chalk_right  
    cmake-args: -DCONFIG_ZMK_SPLIT=y -DCONFIG_ZMK_SPLIT_ROLE_CENTRAL=n
++  group: peripheral

  - board: xiao_ble 
    shield: settings_reset
++  group: reset

  - board: zongle 
    shield: settings_reset
++  group: reset
```

Only build targets of a certain group
```bash
lfz -g central
```


[reset](https://github.com/user-attachments/assets/1b6a464b-f126-4c9a-87d1-2e9ac65cba8a)   


[central](https://github.com/user-attachments/assets/c203e0e9-5979-4109-861b-fca16726bd39)

Defaults to all groups.

### Composition of build arguments

[composition](https://github.com/user-attachments/assets/ddf27544-fa4d-4761-bb9e-9559b7362e3f)


#### See all subcommands
```bash
lfz --help
```

