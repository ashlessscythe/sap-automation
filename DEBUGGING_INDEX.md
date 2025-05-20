# SAP Automation Debugging Guide

This document serves as an index to the debugging resources available for the SAP automation program.

## Available Resources

1. **[DEBUG.md](DEBUG.md)** - Main debugging guide

   - Launch configurations explanation
   - How to start debugging
   - Useful breakpoint locations
   - General debugging tips
   - SAP connection troubleshooting
   - Mock mode vs. Real mode explanation

2. **[BREAKPOINT_EXAMPLES.md](BREAKPOINT_EXAMPLES.md)** - Specific examples of where to place breakpoints

   - VL06O export functionality debugging
   - SAP connection issue debugging
   - Date update functionality debugging
   - Using conditional breakpoints
   - Using logpoints

3. **[TROUBLESHOOTING.md](TROUBLESHOOTING.md)** - Solutions for common issues
   - SAP connection failures
   - File export issues
   - SAP GUI interaction problems
   - Configuration issues
   - Date format issues
   - Excel integration issues
   - Advanced troubleshooting techniques

## Quick Start Guide

### To Start Debugging:

1. Open VS Code
2. Set breakpoints in your code (see [BREAKPOINT_EXAMPLES.md](BREAKPOINT_EXAMPLES.md) for suggestions)
3. Press `F5` or select "Run > Start Debugging" from the menu
4. Choose either "Debug executable 'sap_automation'" or "Debug with SAP Mock"

### If You Encounter Issues:

1. Check [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for common problems and solutions
2. Use the specific breakpoint examples in [BREAKPOINT_EXAMPLES.md](BREAKPOINT_EXAMPLES.md) to isolate the issue
3. Refer to [DEBUG.md](DEBUG.md) for general debugging techniques

## Launch Configurations

The project includes two debug configurations:

1. **Debug executable 'sap_automation'** - For debugging with a real SAP connection
2. **Debug with SAP Mock** - For debugging with mock SAP implementations

To switch between configurations, use the dropdown menu in the Run and Debug panel.

## Key Files for Debugging

- **src/main.rs** - Program entry point and SAP connection initialization
- **src/vl06o.rs** - VL06O transaction implementation
- **src/utils/sap_file_utils.rs** - File operations for SAP exports
- **.vscode/launch.json** - Debug configuration settings

## Additional Tips

- Use the Debug Console (Ctrl+Shift+Y) to evaluate expressions during debugging
- Add variables to the Watch panel to monitor their values
- Use conditional breakpoints for targeted debugging
- Enable logging by uncommenting `pretty_env_logger::init()` in main.rs
