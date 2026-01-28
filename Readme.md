### L(ocal) F(first) Z(MK builds)
[normal](https://github.com/user-attachments/assets/4f39fc48-3006-4dc9-95bc-1233b39ab402)
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
[incremental](https://github.com/user-attachments/assets/df47f49e-7141-4aa5-8ab3-df630024ae77)

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


[reset](https://github.com/user-attachments/assets/6e72389b-4bc7-4e52-9ae5-efe6714b3b4c)   


[central](https://github.com/user-attachments/assets/7a7ed411-d8b5-4947-899d-bb8e1f943a14)

Defaults to all groups.




#### See all subcommands
```bash
lfz --help
```

