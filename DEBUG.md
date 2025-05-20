# Debugging the SAP Automation Program

This guide explains how to debug the SAP automation program using Visual Studio Code.

## Launch Configurations

The project includes two debug configurations in `.vscode/launch.json`:

1. **Debug executable 'sap_automation'** - Standard debugging configuration that builds and runs the program with the real SAP implementation.

2. **Debug with SAP Mock** - Debugging configuration that uses mock SAP implementations, allowing you to test and debug without an actual SAP connection.

## How to Start Debugging

1. Open VS Code and ensure you have the project loaded.
2. Set breakpoints in your code by clicking in the gutter (left margin) next to the line numbers.
3. Open the Debug view by clicking on the Run and Debug icon in the sidebar or pressing `Ctrl+Shift+D`.
4. Select the desired debug configuration from the dropdown at the top of the Debug view.
5. Click the green "Start Debugging" button or press `F5`.

## Useful Breakpoint Locations

Consider setting breakpoints at these key locations:

- **src/main.rs**: At the beginning of the `main()` function to debug program initialization.
- **src/main.rs**: Inside the main menu loop to debug user selections.
- **src/vl06o.rs**: At the beginning of functions like `run_export()`, `run_export_delivery_packages()`, or `run_date_update()` to debug SAP transaction operations.
- **src/utils/sap_file_utils.rs**: In the `save_sap_file()` function to debug file export issues.

## Debugging Tips

1. **Use the Debug Console**: When execution stops at a breakpoint, use the Debug Console to evaluate expressions and inspect variables.

2. **Watch Variables**: Add variables to the Watch panel to monitor their values as the program executes.

3. **Step Through Code**: Use the debug controls to:

   - Step Over (`F10`): Execute the current line and move to the next line.
   - Step Into (`F11`): Step into a function call.
   - Step Out (`Shift+F11`): Complete the current function and return to the caller.
   - Continue (`F5`): Resume execution until the next breakpoint.

4. **Conditional Breakpoints**: Right-click on a breakpoint and select "Edit Breakpoint" to add conditions. For example, to break only when a specific delivery number is being processed.

5. **Logpoints**: Instead of traditional breakpoints, you can add logpoints that print messages to the console without stopping execution. Right-click the gutter and select "Add Logpoint".

## Debugging SAP Connection Issues

If you're experiencing issues with SAP connectivity:

1. Use the "Debug with SAP Mock" configuration to verify your code logic works without SAP.
2. Set breakpoints in the SAP connection initialization code in `main.rs` to see where connection failures occur.
3. Check error messages in the Debug Console when connection attempts fail.

## Mock Mode vs. Real Mode

- **Mock Mode**: Uses simulated SAP responses, useful for testing logic without an actual SAP system.
- **Real Mode**: Connects to a real SAP system, requires proper SAP GUI setup and permissions.

When using mock mode, the program will set the `SAP_MOCK` environment variable to "true" and compile with the "mock" feature flag.

## Common Issues

1. **SAP GUI Scripting Not Enabled**: Ensure SAP GUI scripting is enabled in your SAP system.
2. **Permission Issues**: Verify you have the necessary permissions for the SAP transactions you're automating.
3. **Path Issues**: Check that file paths for exports are valid and accessible.

## Advanced Debugging

For more complex issues:

1. **Memory Inspection**: Use the Memory view to inspect raw memory when debugging low-level issues.
2. **Disassembly View**: For performance optimization, examine the disassembled code.
3. **Debug Output**: Add temporary `println!()` statements to track program flow.
