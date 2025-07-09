Attribute VB*Name = "Control_id*"
Option Explicit

Public Const time_form = "mm-dd-yy hh:mm:ss"
Public Const str_form = vbCrLf & "********\*\*\*\*********\*\*\*\*********\*\*\*\*********\*\*********\*\*\*\*********\*\*\*\*********\*\*\*\*********" & vbCrLf
Public Type Wnd_Title_Caption
Wnd_Type As String
Wnd_Title As String
End Type

Public Type error_check
bchgb As Boolean
msg As String
End Type

Public Type ctrl_check
cband As Boolean
ctext As String
ctype As String
End Type

Public Type Params_Struc
nParam(1 To 10) As String
End Type

Public Type params_struct
clientID As String
user As String
pass As String
language As String
End Type

Const WM_CLOSE = &H10
Const Word = "OpusApp"
Const Excel = "XLMAIN"
Const IExplorer = "IEFrame"
Const MSVBasic = "wndclass_desked_gsk"
Const NotePad = "Notepad"
'Struct To Define General Parameters to Connect/Disconnect Net Resources
Private Type NETRESOURCE
dwScope As Long
dwType As Long
dwDisplayType As Long
dwUsage As Long
lpLocalName As String
lpRemoteName As String
lpComment As String
lpProvider As String
End Type

'Define the Resource
Enum RESOURCE
R_CONNECTED = &H1
R_REMEMBERED = &H3
R_GLOBALNET = &H2
End Enum

'Define the Resource Type
Enum RESOURCE_TYPE
R_DISK = &H1
R_PRINT = &H2
R_ANY = &H0
End Enum

'Define the View Type
Enum RESOURCE_VIEWTYPE
R_DOMAIN = &H1
R_GENERIC = &H0
R_SERVER = &H2
R_SHARE = &H3
End Enum

'Define the Resource Use
Enum RESOURCE_USETYPE
R_CONNECTABLE = &H1
R_CONTAINER = &H2
End Enum

' The following includes all the constants defined for NETRESOURCE,
' not just the ones used in this example.
Private Const CONNECT_UPDATE_PROFILE = &H1
'Constant for Resource
Private Const RESOURCE_CONNECTED = &H1
Private Const RESOURCE_REMEMBERED = &H3
Private Const RESOURCE_GLOBALNET = &H2
'Constant for Resource Type
Private Const RESOURCETYPE_DISK = &H1
Private Const RESOURCETYPE_PRINT = &H2
Private Const RESOURCETYPE_ANY = &H0
'Constant for Resource View Type
Private Const RESOURCEDISPLAYTYPE_DOMAIN = &H1
Private Const RESOURCEDISPLAYTYPE_GENERIC = &H0
Private Const RESOURCEDISPLAYTYPE_SERVER = &H2
Private Const RESOURCEDISPLAYTYPE_SHARE = &H3
'Constant for Resource Use
Private Const RESOURCEUSAGE_CONNECTABLE = &H1
Private Const RESOURCEUSAGE_CONTAINER = &H2
' Error Constants:
Private Const NO_ERROR = 0
Private Const ERROR_ACCESS_DENIED = 5&
Private Const ERROR_BAD_DEV_TYPE = 66&
Private Const ERROR_BAD_NET_NAME = 67&
Private Const ERROR_ALREADY_ASSIGNED = 85&
Private Const ERROR_INVALID_PASSWORD = 86&
Private Const ERROR_BUSY = 170&
Private Const ERROR_BAD_DEVICE = 1200&
Private Const ERROR_BAD_PROFILE = 1206&
Private Const ERROR_DEVICE_ALREADY_REMEMBERED = 1202&
Private Const ERROR_NO_NET_OR_BAD_PATH = 1203&
Private Const ERROR_BAD_PROVIDER = 1204&
Private Const ERROR_CANNOT_OPEN_PROFILE = 1205&
Private Const ERROR_EXTENDED_ERROR = 1208&
Private Const ERROR_CANCELLED = 1223&

Public Declare PtrSafe Function WNetAddConnection2 Lib "mpr.dll" Alias "WNetAddConnection2A" (lpNetResource As NETRESOURCE, ByVal lpPassword As String, ByVal lpUserName As String, ByVal dwFlags As Long) As Long
Public Declare PtrSafe Function WNetCancelConnection2 Lib "mpr.dll" Alias "WNetCancelConnection2A" (ByVal lpName As String, ByVal dwFlags As Long, ByVal fForce As Long) As Long
Public Declare PtrSafe Function WNetGetUser Lib "Mpr" Alias "WNetGetUserA" (ByVal lpName As String, ByVal lpUserName As String, lpnLength As Long) As Long
'******\*\*\*\*******\*\*******\*\*\*\*******\*\*\*******\*\*\*\*******\*\*******\*\*\*\*******
Public Declare PtrSafe Function FindWindow Lib "User32" Alias "FindWindowA" (ByVal lpClassName As String, ByVal lpWindowName As String) As Long
Public Declare PtrSafe Function GetWindowText Lib "User32" Alias "GetWindowTextA" (ByVal hWND As Long, ByVal lpString As String, ByVal cch As Long) As Long
Public Declare PtrSafe Function GetWindowTextLength Lib "User32" Alias "GetWindowTextLengthA" (ByVal hWND As Long) As Long
Public Declare PtrSafe Function GetWindow Lib "User32" (ByVal hWND As Long, ByVal wCmd As Long) As Long
Public Declare PtrSafe Function IsWindowVisible Lib "User32" (ByVal hWND As Long) As Boolean

Public Const GW_HWNDNEXT = 2
Public Declare PtrSafe Function SetWindowPos Lib "User32" (ByVal hWND As Long, ByVal hWndInsertAfter As Long, ByVal x As Long, ByVal y As Long, ByVal cx As Long, ByVal cy As Long, ByVal wFlags As Long) As Long
Public Const SWP_HIDEWINDOW = &H80
Public Const SWP_SHOWWINDOW = &H40

Private Declare PtrSafe Function SendMessage Lib "User32" Alias "SendMessageA" (ByVal hWND As Long, ByVal wMsg As Long, ByVal wParam As Long, lParam As Any) As Long
'******\*\*\*\*******\*\*******\*\*\*\*******\*\*\*******\*\*\*\*******\*\*******\*\*\*\*******
Public Declare PtrSafe Function ShellExecute Lib "shell32.dll" Alias "ShellExecuteA" (ByVal hWND As Long, ByVal lpOperation As String, ByVal lpFile As String, ByVal lpParameters As String, ByVal lpDirectory As String, ByVal nShowCmd As Long) As Long

Function assert_tcode(ByVal sess, ByVal tCode, Optional ByVal wnd = 0) As Boolean
Dim msg As String, err_ctrl As ctrl_check, err_msg As String

    With sess
         .StartTransaction tCode
        err_msg = Hit_Ctrl(sess, wnd, "/sbar", "Text", "Get", "")
        If Mult_Contains(err_msg, "exist", "autho") Then
            ' print msg
            msg = err_msg
            assert_tcode = False
            GoTo errlv
        End If

        If Len(err_msg) < 1 Then
            assert_tcode = True
            Exit Function
        End If

    End With

    Exit Function

errlv:
dbLog.log msg, msgPopup:=True, msgType:=vbExclamation
End Function

Function Check_Export_Window(session, tCode, CorrectTitle As String) As Boolean
Dim err_wnd As ctrl_check
Dim ctrl_id As String
Dim base_obj_id As String
Dim obj_id As String

    With session
        If Not check_tcode(session, tCode, 0, 0) Then
            dbLog.log "tCode (" & tCode & ") not active, exiting...."
            GoTo errlv
        End If

redo:
err_wnd = Exist_Ctrl(session, 1, "", True)
Select Case err_wnd.cband
Case True
Debug.Print "Looking for: " & CorrectTitle

            Select Case True
            Case Contains(err_wnd.ctext, "SELECT SPREADSHEET")
                .FindById("wnd[1]/tbar[0]/btn[0]").press ' Excel
                GoTo exitTrue
            Case Contains(err_wnd.ctext, "SAVE LIST IN FILE...")
                Debug.Print "Window Title " & err_wnd.ctext
                Debug.Print "Saving as 'local file'"
                .FindById("wnd[1]/usr/subSUBSCREEN_STEPLOOP:SAPLSPO5:0150/sub:SAPLSPO5:0150/radSPOPLI-SELFLAG[1,0]").Select
                .FindById("wnd[1]/tbar[0]/btn[0]").press

                GoTo exitTrue
            Case Contains(err_wnd.ctext, CorrectTitle)                   ' exit select/case
                GoTo exitTrue
            Case Else
                Debug.Print "Error with Excel Export, trying Local File, trying to correct..."
                Debug.Print "Window title " & err_wnd.ctext
            End Select
        Case False
            ' tCode Specific to get export window if non-existent
            ' goto redo if (right-click > Spreadsheet) with no grid object, else grid obj below
            Select Case tCode
            Case "VT11"
                base_obj_id = ""
                obj_id = "wnd[0]" & base_obj_id
                err_wnd = Exist_Ctrl(session, 0, base_obj_id, True)
                .FindById("wnd[0]").SendVKey 44
                GoTo redo                        ' no need for grid obj
            Case "MB51"
                base_obj_id = "/usr/cntlGRID1/shellcont/shell"
                obj_id = "wnd[0]" & base_obj_id
                err_wnd = Exist_Ctrl(session, 0, base_obj_id, True)
            Case "ZWM_MDE_COMPARE"
                base_obj_id = "/usr/cntlGRID1/shellcont/shell"
                obj_id = "wnd[0]" & base_obj_id
                err_wnd = Exist_Ctrl(session, 0, base_obj_id, True)
            Case "ZMDESNR"
                base_obj_id = "/usr/cntlGRID1/shellcont/shell/shellcont[1]/shell"
                obj_id = "wnd[0]" & base_obj_id
                err_wnd = Exist_Ctrl(session, 0, base_obj_id, True)
            Case Else
                dbLog.log "Grid for TCODE (" & tCode & ") needs to be set up in Check_Export_Window"
            End Select

            If err_wnd.cband Then
                .FindById(obj_id).SelectedRows = "0"
                .FindById(obj_id).CurrentCellRow = -1
                .FindById(obj_id).contextmenu
                .FindById(obj_id).selectContextMenuItem "&XXL"
                .FindById("wnd[1]/usr/chkCB_ALWAYS").Selected = False
                GoTo redo
            End If
        End Select
    End With

    Exit Function

exitTrue:
Check_Export_Window = True
Exit Function

errlv:
Check_Export_Window = False
End Function

Function check_multi_paste(sess, tCode, nWnd, nRow) As Boolean
Dim err_ctl As ctrl_check
Dim a, t, obj, vN, limit
limit = nRow

    For Each obj In Array _
        ("/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/txtRSCSEL_255-SLOW_I[1," & nRow & "]", _
         "/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1," & nRow & "]", _
         "/usr/tabsTAB_STRIP/tabpNOSV/ssubSCREEN_HEADER:SAPLALDB:3030/tblSAPLALDBSINGLE_E/txtRSCSEL_255-SLOW_E[1," & nRow & "]", _
         "/usr/tabsTAB_STRIP/tabpNOSV/ssubSCREEN_HEADER:SAPLALDB:3030/tblSAPLALDBSINGLE_E/ctxtRSCSEL_255-SLOW_E[1," & nRow & "]")
        With sess
            err_ctl = Exist_Ctrl(sess, nWnd, obj, True)
            If err_ctl.cband Then
                t = err_ctl.ctext
                Debug.Print "Text in row (" & nRow & ") is " & Len(t)
                ' Check if non-zero string
                If Len(t) > 0 Then
                    a = True
                Else
                    ' If zero, check next row if also blank
                    limit = limit + 1
                    If limit < 2 Then
                        a = check_multi_paste(sess, tCode, nWnd, limit)
                        If Not a Then GoTo errlv Else Exit Function
                    End If
                End If
            End If
        End With
    Next

    check_multi_paste = a

    If check_multi_paste = True Then
        dbLog.log str_form & "Paste confirmed. Row (" & nRow & ") is not empty" & str_form
    End If
    Exit Function

errlv:

End Function

Function check_select_layout(ByVal sess, ByVal tCode, ByVal LayoutRow, Optional ByVal arg As Object, \*
Optional ByVal bRunPre As Boolean = False) As params
Dim err_ctl As ctrl_check, err_msg As String, bar_msg As String, ctrl_msg As String
Dim local_rVal As params
With sess

        Application.StatusBar = "Checking / selecting layout for tCode (" & tCode & ")"

        If Not arg Is Nothing Then
            ' // optional send F3 prior to export
            If arg.Exists("pre_export_back") Then
                If arg("pre_export_back") = True Then .FindById("wnd[0]").SendVKey 3
            End If
            ' // get layoutrow from arg
            If arg.Exists("layout") Then
                LayoutRow = arg("layout")
            End If
        End If

        ' /// pre stuff
        If bRunPre Then
            Dim err_wnd As ctrl_check
            ' Layout
            If Not IsMissing(LayoutRow) Then
                If Contains(LayoutRow, "layout") Then LayoutRow = Replace(Split(LayoutRow, " ")(0), "layout:", "")
            ElseIf IsMissing(LayoutRow) And arg.Exists("layout") Then
                LayoutRow = arg("layout")
            ElseIf Len(arg) < 1 Then
                ' missing
                LayoutRow = ""
            ElseIf Not IsMissing(LayoutRow) Then
                LayoutRow = Replace(LayoutRow, "layout:", "")
            Else
                ' close popup (if exists)
                err_wnd = Exist_Ctrl(sess, 1, "", True)
                If err_wnd.cband Then
                    .FindById("wnd[1]").Close
                End If
                LayoutRow = ""
            End If
        End If

wndCheck:
If Not IsMissing(LayoutRow) And Len(LayoutRow) > 1 Then
Select Case LCase(tCode)
Case "lx03", "lx02", "lt23", "vt22"
.FindById("wnd[0]/tbar[1]/btn[33]").press ' Select Layout
Case "vt11"
.FindById("wnd[0]/mbar/menu[3]/menu[0]/menu[1]").Select ' Choose Layout Button
Case "zmdesnr", "mb52"
If Exist_Ctrl(sess, 0, "/tbar[1]/btn[33]", True).cband Then
.FindById("wnd[0]/tbar[1]/btn[33]").press
End If
Case Else
End Select
' check for error /sbar
bar_msg = Hit_Ctrl(sess, 0, "/sbar", "Text", "Get", "")
If Contains(bar_msg, "valid function") Then
GoTo errlv
End If
Select Case True
Case IsEmpty(LayoutRow) Or Len(LayoutRow) = 0
' If layout is empty or zero-length, close popup window and export as-is
If Exist_Ctrl(sess, 1, "", True).cband Then
.FindById("wnd[1]").Close
End If
dbLog.log "Layout (" & LayoutRow & ") is empty or zero-length. " & vbCrLf & "Exporting as-is."

            Case IsNumeric(LayoutRow)
                err_ctl = Exist_Ctrl(sess, 1, "/usr/lbl[1," & LayoutRow & "]", True)
                If Not err_ctl.cband Then
                    dbLog.log "Layout number (" & LayoutRow & ") not found, decreasing"
                    LayoutRow = LayoutRow - 1
                    GoTo wndCheck
                Else
                    err_msg = err_ctl.ctext
                    .FindById("wnd[1]/usr/lbl[1," & LayoutRow & "]").SetFocus ' Highlight LayoutRow
                    .FindById("wnd[1]").SendVKey 2 ' Select
                    dbLog.log "Layout number (" & LayoutRow & "), (" & err_msg & ") selected."
                    local_rVal.run_check = True
                End If

            Case Else
                ' Check if window exists
                err_ctl = Exist_Ctrl(sess, 1, "", True)
                ' Above window might not exist if no layouts are set up for user
                Dim objName
                If err_ctl.cband Then
                    If Contains(err_ctl.ctext, "change layout") Then
                        GoTo setup
                    ElseIf Contains(err_ctl.ctext, "choose") Then
                        GoTo choose
                    End If

choose:
' SELECT LAYOUT
' objName = "wnd[1]/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell"'
Select Case LCase(tCode)
Case "zvt11", "zmdesnr"
objName = "/usr/ssubD0500_SUBSCREEN:SAPLSLVC_DIALOG:0501/cntlG51_CONTAINER/shellcont/shell"
local_rVal.run_check = SelectLayout(sess, 1, objName, LayoutRow)
Case Else
bar_msg = choose_layout(sess, tCode, LayoutRow)
End Select
' Check statusbar msg
If Len(bar_msg) < 1 Then bar_msg = Hit_Ctrl(sess, 0, "/sbar", "Text", "Get", "")
' Check Statusbar message to check if layout found above ' Updated 181105
If Contains(bar_msg, "No Layout", 0) Then
GoTo setup
ElseIf Contains(bar_msg, "applied") Then GoTo layoutFound
End If
Else
' window doesn't exist need to setup (no layouts exist?)
GoTo setup
End If
' If run_check Then GoTo layoutFound
' Loop through available saved layouts (Still needs work)

                Dim i
                If err_ctl.cband Then
                    For i = 1 To 60              ' layouts start at 3
                        err_ctl = Exist_Ctrl(sess, 1, "/usr/lbl[1," & i & "]", True)
                        If err_ctl.cband Then
                            ctrl_msg = .FindById("wnd[1]/usr/lbl[1," & i & "]").text
                            If UCase(ctrl_msg) = UCase(LayoutRow) Then
                                .FindById("wnd[1]/usr/lbl[1," & i & "]").SetFocus
                                .FindById("wnd[1]").SendVKey 2 ' Select
                                dbLog.log "Layout number (" & i & "), (" & LayoutRow & ") selected."
                                local_rVal.run_check = True
                                Exit For         ' Exit loop if layout found
                            End If
                        End If
                    Next

setup:
' Check Statusbar message to check if layout found above ' Updated 181105
Dim bNoSave As Boolean: bNoSave = False
If arg.Exists("nosave") Then bNoSave = True
bar*msg = Hit_Ctrl(sess, 0, "/sbar", "Text", "Get", "")
If Not Contains(bar_msg, "layout applied", 0) Then
' Current: If layout not found, close popup window and Setup_Layout_li()
' Future: if string layout not found, set it up.
err_ctl = Exist_Ctrl(sess, 1, "", True)
If err_ctl.cband Then
.FindById("wnd[1]").Close
End If ' Updated 181105
dbLog.log "Layout (" & LayoutRow & ") not found." & vbCrLf & "Setting up layout " & Format(Now, time_form)
Dim listRows As New Collection
Call Populate_Collection_Vertical(tWB, "Layouts", LCase(LayoutRow & "*" & tCode & "\_layoutrows"), listRows)
Debug.Print listRows.Count
Select Case LCase(tCode)
Case "zmdesnr", "zvt11"
.FindById("wnd[0]/tbar[1]/btn[32]").press '

                            Call SetupLayout(sess, 1, "/usr/tabsG_TS_ALV/tabpALV_M_R1/ssubSUB_DYN0510:SAPLSKBH:0620", _
                                             LayoutRow, listRows, 200, bNoSave)
                        Case "vt11"
                            dbLog.log "Layout (" & LayoutRow & ") not found." & vbCrLf & "Setting up layout " & Format(Now, time_form)
                            Call Populate_Collection_Vertical(tWB, "Layouts", LCase(LayoutRow & "_" & tCode & "_layoutrows"), listRows, 0)
                            Debug.Print listRows.Count
                            .FindById("wnd[0]/mbar/menu[3]/menu[0]/menu[0]").Select ' Open Current Layout
                            Call SetupLayout_li _
                                  (sess, tCode, 1, "/usr/tabsTS_LINES/tabpLI01/ssubSUB810:SAPLSKBH:0810", LayoutRow, listRows, 200, bNoSave)
                        Case Else
                            .FindById("wnd[0]/mbar/menu[3]/menu[2]/menu[0]").Select ' Open Current Layout
                            Call SetupLayout_li _
                                 (sess, tCode, 1, "/usr/tabsTS_LINES/tabpLI01/ssubSUB810:SAPLSKBH:0810", LayoutRow, listRows, 200, bNoSave)
                        End Select

                        local_rVal.run_check = True
                    End If
                End If
            End Select
        End If

        ' Check Statusbar message to check if layout found above                ' Updated 181105
        bar_msg = Hit_Ctrl(sess, 0, "/sbar", "Text", "Get", "")
        If Contains(bar_msg, "Layout", 0) Then
            dbLog.log "Sbar msg: (" & bar_msg & "). " & Format(Now, time_form)
        End If

layoutFound:
' Make sure window dissappear
err_ctl = Exist_Ctrl(sess, 1, "", True)

        If err_ctl.cband Then
            .FindById("wnd[1]").Close
        End If
        '''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''

        Dim export_wnd_name As String, msg
        ' List > Export > Spreadsheet
        Select Case LCase(tCode)
        Case "lx03", "lx02"
            .FindById("wnd[0]/mbar/menu[0]/menu[1]/menu[1]").Select
            export_wnd_name = "BIN STATUS REPORT: OVERVIEW"
        Case "vt11"
            ' check for no results
            err_wnd = Exist_Ctrl(sess, 0, "/usr/lbl[2,4]", 1)
            If err_wnd.cband Then
                ' get text
                msg = Hit_Ctrl(sess, 0, "/usr/lbl[2,4]", "Text", "Get", "")
                If Contains(msg, "no data") Then GoTo errlv
            End If
            .FindById("wnd[0]/mbar/menu[0]/menu[10]/menu[0]").Select ' Export to Excel
            export_wnd_name = "SHIPMENT LIST: PLANNING"
        Case "zmdesnr", "zvt11"
            .FindById("wnd[0]/mbar/menu[0]/menu[3]/menu[1]").Select
            export_wnd_name = "ZMDEMAIN Serial Number History contents"
        Case "vl06o"
            .FindById("wnd[0]/mbar/menu[0]/menu[5]/menu[1]").Select ' Export as Excel
            ''''''''''''Check Export Window''''''''''''''''''
            export_wnd_name = "LIST OF OUTBOUND DELIVERIES"
        End Select
        '''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''
        ''''''''''''Check Export Window Name ''''''''''''''''''''''''''
        ''''''''''''Check Export Window Name ''''''''''''''''''''''''''
        local_rVal.run_check = Check_Export_Window(sess, tCode, export_wnd_name)
        If Not local_rVal.run_check Then GoTo errlv
        '''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''
        local_rVal.type = "export_window"
    End With

out:
Application.StatusBar = False

    check_select_layout = local_rVal

    Exit Function

errlv:
local_rVal.run_check = False
local_rVal.err = bar_msg
GoTo out
End Function

Function check_tcode(session, tCode, Optional run = True, Optional killpopups = False) As Boolean
Dim cur

    dbLog.log "Checking if tCode (" & tCode & ") is active"
    cur = session.Info.Transaction

    ' Check if on tCode
    If Contains(cur, tCode, 0) Then
        dbLog.log "tCode (" & tCode & ") is active"
        check_tcode = True
    Else
        If run Then
            ' run if requested
            dbLog.log "tCode mismatch, attempting to run tCode (" & tCode & ")"
            Evaluate assert_tcode(session, tCode)
            Time_Event
            ' Loop self to check
            Call check_tcode(session, tCode)
        Else
            GoTo errlv
        End If
    End If

    ' if option to killpopups is set then re-start tcode
    If killpopups Then
        dbLog.log "Option to killpopups passed, restarting tCode (" & tCode & ")"
        Evaluate assert_tcode(session, tCode)
        Time_Event
        ' Loop self to check
        Call check_tcode(session, tCode)
    End If

    Exit Function

errlv:
dbLog.log "tCode mismatch. Current tCode is (" & cur & "), need (" & tCode & ")"
End Function

'=============================================================================================================
' Check_wnd
'
' This Function Get the SAP GUI Window Type and Run the Options according the Window Title
'
' Parameter : Type : Use:
' ----------------------- --------------- --------------------------------------------------------------------
' Session Object SAP Session (1...6), by Default is 1st Session
' nWnd Integer SAP Window Id (Parent=0,Child=1...n)
' Params_In Params_Struc Input User Parameters
'
' Return :
' ----------------------- ------------------------------------------------------------------------------------
' error_check True/False, Msg
'
'=============================================================================================================
Function Check_wnd(session, nWnd, params_in As params_struct) As error_check
Dim retid
Dim aux_str As String
Dim RetValue As Boolean
Dim err_chk As error_check
Dim err_wnd As ctrl_check
Dim err_ctl As ctrl_check
Dim ctrl_id, Hit_Ctrl_Aux As String
Dim get_time As String
Dim wnd_aux As Wnd_Title_Caption

    get_time = Format(Now(), "mm-dd-yy hh:mm:ss")

    err_wnd = Exist_Ctrl(session, nWnd, "", True)
    If err_wnd.cband = True Then
        wnd_aux.Wnd_Type = err_wnd.ctype
        wnd_aux.Wnd_Title = err_wnd.ctext
        Select Case (wnd_aux.Wnd_Type)
        Case "GuiFrameWindow":
        Case "GuiMainWindow":
            Select Case (wnd_aux.Wnd_Title)
            Case "SAP Easy Access":              'SAP Menu Initial Screen
                err_chk.bchgb = True
                err_chk.msg = ""
                'Note: Maximize SAP Easy Access Window, 04-14-09.
                err_ctl = Exist_Ctrl(session, nWnd, "", True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, "", "Focus", "", "")
                    aux_str = Hit_Ctrl(session, nWnd, "", "Maximize", "", "")
                End If
            Case "SAP R/3":                      'Login Initial Screen
                ctrl_id = "/usr/txtRSYST-MANDT"  'Client
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Set", params_in.clientID)
                End If
                ctrl_id = "/usr/txtRSYST-BNAME"  'User
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Set", params_in.user)
                End If
                ctrl_id = "/usr/pwdRSYST-BCODE"  'Password
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Set", params_in.pass)
                End If
                ctrl_id = "/usr/txtRSYST-LANGU"  'Language
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Set", params_in.language)
                End If
                ctrl_id = "/tbar[0]/btn[0]"      'Enter
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                End If
                err_chk.bchgb = True
                err_chk.msg = get_time
            Case "SAP":                          'Login Initial Screen (Alternate)
                ctrl_id = "/usr/txtRSYST-MANDT"  'Client
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Set", params_in.clientID)
                End If
                ctrl_id = "/usr/txtRSYST-BNAME"  'User
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Set", params_in.user)
                End If
                ctrl_id = "/usr/pwdRSYST-BCODE"  'Password
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Set", params_in.pass)
                End If
                ctrl_id = "/usr/txtRSYST-LANGU"  'Language
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Set", params_in.language)
                End If
                ctrl_id = "/tbar[0]/btn[0]"      'Enter
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                End If
                err_chk.bchgb = True
                err_chk.msg = get_time
            Case "Stock/Requirements List: Initial Screen": 'MD07
                err_chk.bchgb = True
                err_chk.msg = ""
            Case "Stock/Requirements List: Material List": 'MD07...List Ready to Export before Ctrl + P
                err_chk.bchgb = True
                err_chk.msg = ""
            Case "General Table Display":        'SE16N
                err_chk.bchgb = True
                err_chk.msg = ""
            Case "Display of Entries Found":     'SE16N
                err_chk.bchgb = True
                err_chk.msg = ""
            Case "Start Report":                 'Start Report
                err_chk.bchgb = True
                err_chk.msg = ""
            Case "Display Warehouse Stocks of Material on Hand": 'MB52
                err_chk.bchgb = True
                err_chk.msg = ""
            End Select
        Case "GuiModalWindow":
            Select Case (wnd_aux.Wnd_Title)
            Case "Log Off":
                ctrl_id = "/usr/btnSPOP-OPTION2"
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                End If
                err_chk.bchgb = True
                err_chk.msg = get_time
            Case "System Messages":
                ctrl_id = "/tbar[0]/btn[0]"
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                End If
                err_chk.bchgb = True
                err_chk.msg = get_time
            Case "Create material list":         'MD07
                aux_str = ""
                ctrl_id = "/usr/txtSPOP-DIAGNOSE1" 'Information:
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
                End If
                ctrl_id = "/usr/txtSPOP-DIAGNOSE2" 'Matls were selected
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = aux_str & " " & Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
                End If
                ctrl_id = "/usr/txtSPOP-TEXTLINE1" 'Set up lists beforehand?
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = aux_str & " " & Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
                End If
                ctrl_id = "/usr/btnSPOP-OPTION1" 'Yes
                'Ctrl_Id = "/usr/btnSPOP-OPTION2"    'No
                'Ctrl_Id = "/usr/btnSPOP-OPTION_CAN" 'Cancel
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                End If
                err_chk.bchgb = True
                err_chk.msg = aux_str & " " & get_time
            Case "Print list - variable":        'MD07
                aux_str = ""
                ctrl_id = "/usr/txtMESSTXT1"     'Save the data in the spreadsheet
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
                End If
                ctrl_id = "/tbar[0]/btn[0]"      'Enter
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                End If
                err_chk.bchgb = True
                err_chk.msg = aux_str
            Case "Display of Entries Found":     'SE16N
                aux_str = ""
                ctrl_id = "/usr/txtMESSTXT1"     'Save the data in the spreadsheet
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
                End If
                If aux_str = "Save the data in the spreadsheet" Then
                    ctrl_id = "/tbar[0]/btn[0]"  'Enter
                    err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                    If err_ctl.cband = True Then
                        aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                    End If
                End If
                If aux_str = "Filter criteria, sorting, totals and subtotals are" Then
                    ctrl_id = "/tbar[0]/btn[0]"  'Enter
                    err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                    If err_ctl.cband = True Then
                        aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                    End If
                End If
                err_chk.bchgb = True
                err_chk.msg = aux_str
            Case "Export list object to XXL":
                aux_str = ""
                ctrl_id = "/usr/txtSPOP5-TEXTLINE1" 'An XXL list object is exported with
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
                End If
                If aux_str = "An XXL list object is exported with" Then
                    'Export list object to XXL -> (o)Excel SAP macros //(0)Table
                    ctrl_id = "/usr/sub:SAPLSPO5:0101/radSPOPLI-SELFLAG[0,0]"
                    err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                    If err_ctl.cband = True And Trim(err_ctl.ctext) = "Excel SAP macros" Then
                        Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Select", "Set", "")
                    End If
                    ctrl_id = "/usr/sub:SAPLSPO5:0101/radSPOPLI-SELFLAG[0,0]"
                    err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                    If err_ctl.cband = True And Trim(err_ctl.ctext) = "Table" Then
                        Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Select", "Set", "")
                    End If
                    'Export list object to XXL -> (o)Table
                    ctrl_id = "/usr/sub:SAPLSPO5:0101/radSPOPLI-SELFLAG[1,0]"
                    err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                    If err_ctl.cband = True And Trim(err_ctl.ctext) = "Table" Then
                        Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Select", "Set", "")
                    End If
                    'Enter
                    ctrl_id = "/tbar[0]/btn[0]"
                    err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                    If err_ctl.cband = True Then
                        Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                    End If
                End If
                aux_str = ""
                ctrl_id = "/usr/sub:SAPLSPO5:0101/radSPOPLI-SELFLAG[0,0]" 'Microsoft Excel
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
                End If
                If aux_str = "Microsoft Excel" Then
                    'Enter
                    ctrl_id = "/tbar[0]/btn[0]"
                    err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                    If err_ctl.cband = True Then
                        Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                    End If
                End If
                err_chk.bchgb = True
                err_chk.msg = aux_str
            Case "Exit overview":                'MD07
                aux_str = ""
                ctrl_id = "/usr/txtSPOP-TEXTLINE1" 'Data selection is deleted when you exit the overview
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
                End If
                ctrl_id = "/usr/btnSPOP-OPTION1" 'Yes
                'Ctrl_Id = "/usr/btnSPOP-OPTION2"    'No
                'Ctrl_Id = "/usr/btnSPOP-OPTION_CAN" 'Cancel
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    Hit_Ctrl_Aux = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                End If
                err_chk.bchgb = True
                err_chk.msg = aux_str
            Case "Material Master General Information": 'Report Title
                aux_str = ""
                ctrl_id = "/usr/txtMESSTXT1"     'Save the data in the spreadsheet
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
                End If
                ctrl_id = "/tbar[0]/btn[0]"      'Enter
                err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
                If err_ctl.cband = True Then
                    aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Press", "", "")
                End If
                err_chk.bchgb = True
                err_chk.msg = aux_str
            End Select
        End Select
    End If
    Check_wnd = err_chk

End Function

Function choose_layout(ByVal sess, ByVal tCode, ByVal LayoutRow) As String
Dim err_wnd As ctrl_check, msg As String
With sess
' check if window exists
err_wnd = Exist_Ctrl(sess, 1, "", 1)
If Not err_wnd.cband Then Evaluate layout_popup(sess, tCode)

        ' check title?
        Select Case True
        Case Contains(LCase(err_wnd.ctext), "choose")
            ' stuff
        Case Contains(LCase(err_wnd.ctext), "change")

        End Select

        ' Find
        .FindById("wnd[1]/tbar[0]/btn[71]").press
        ' checkbox
        If Exist_Ctrl(sess, 2, "/usr/chkSCAN_STRING-START", 1).cband Then
            .FindById("wnd[2]/usr/chkSCAN_STRING-START").Selected = False
        End If
        ' layoutname
        .FindById("wnd[2]/usr/txtRSYSF-STRING").text = LayoutRow
        ' Enter
        .FindById("wnd[2]/tbar[0]/btn[0]").press
        ' check wnd 3
        err_wnd = Exist_Ctrl(sess, 3, "", 1)
        If err_wnd.cband Then
            ' check result exists
            err_wnd = Exist_Ctrl(sess, 3, "/usr/lbl[1,2]", 1)
            If err_wnd.cband Then
                ' highlight
                .FindById("wnd[3]/usr/lbl[1,2]").SetFocus
                ' click
                .FindById("wnd[3]").SendVKey 2
            Else
                ' err info window
                msg = "No Layout"
                Evaluate close_popups(sess)
                GoTo out
            End If
        End If                                   ' 3 popup

        ' enter (close win)
        .FindById("wnd[1]/tbar[0]/btn[0]").press
        ' check wnd closed
        err_wnd = Exist_Ctrl(sess, 1, "", 1)
        If err_wnd.cband Then .FindById("wnd[1]").Close

        ' get sbar
        msg = Hit_Ctrl(sess, 0, "/sbar", "Text", "Get", "")
    End With

out:
dbLog.log msg
choose_layout = msg
Exit Function
errlv:

End Function

Function choose_variant(ByVal sess As GuiSession, ByVal tCode As String, ByVal sap_var As String) As Boolean

    With sess
        Evaluate assert_tcode(sess, tCode)
        ' choose variant
        .FindById("wnd[0]").SendVKey 17
        ' Check window title
        If Contains(Exist_Ctrl(sess, 1, "", True).ctext, "abap") Then
            ' Abap variant select
            Dim ret_str As String

            ' loop
            Dim i: i = 0
            Do

            ret_str = Hit_Ctrl(sess, 1, "/usr/cntlALV_CONTAINER_1/shellcont/shell", "GetCellValue", "VARIANT", i)
            Dim shl As GuiGridView
            Evaluate LetOrSetElement(shl, sess.FindById("wnd[1]/usr/cntlALV_CONTAINER_1/shellcont/shell"))
            Debug.Print TypeName(shl)

            If Contains(ret_str, sap_var) Then
                ' select when found
                shl.SetCurrentCell i, "VARIANT"
                shl.DoubleClickCurrentCell
                choose_variant = True
                dbLog.log "Variant (" & sap_var & ") selected."
                GoTo out
                Exit Do
            End If
            i = i + 1
            Loop While ret_str <> ""
            GoTo errlv

        Else
            ' traditional variant select
            .FindById("wnd[1]/usr/txtV-LOW").text = sap_var
            ' clear name
            .FindById("wnd[1]/usr/txtENAME-LOW").text = ""
            ' enter
            .FindById("wnd[1]").SendVKey 0
            ' close
            .FindById("wnd[1]").SendVKey 8
            ' if wnd exists
            If Exist_Ctrl(sess, 1, "", True).cband Then GoTo errlv
            choose_variant = True
        End If
    End With

out:

    Exit Function

errlv:

If Exist_Ctrl(sess, 1, "", True).cband Then
Dim msg: msg = get_sap_text_errors(sess, 1, "/usr/txtMESSTXT1", 10)
If Len(msg) < 1 Then dbLog.log "Variant not found or selected.", msgPopup:=True, msgType:=vbCritical
End If

End Function

Function get_sap_lbl_data(sess As Object, start_x As Integer, end_x As Integer, start_y As Integer, end_y As Integer) As Variant
Dim ba As New BetterArray
Dim i As Integer, j As Integer
Dim labelId As String
Dim lbl As Object
Dim dataRow As Boolean
Dim headerCaptured As Boolean
Dim rowCounter As Integer
Dim colCounter As Integer

    ' Initialize the BetterArray (assuming you have a BetterArray class or module)
    rowCounter = 1
    headerCaptured = False

    ' Loop through the specified range
    For i = start_y To end_y
        dataRow = False
        colCounter = 1

        For j = start_x To end_x
            ' Construct the label ID dynamically
            labelId = "wnd[0]/usr/lbl[" & j & "," & i & "]"
            On Error Resume Next
            Set lbl = sess.FindById(labelId)
            On Error GoTo 0

            ' Check if the label exists
            If Not lbl Is Nothing Then
                ' Check if the first item in the row matches the shipment number pattern
                If j = start_x And lbl.text Like "00##########" Then
                    dataRow = True
                End If

                ' If it's a header row and not yet captured
                If Not headerCaptured And Not dataRow Then
                    ba.Push lbl.text
                    colCounter = colCounter + 1
                ' If it's a data row
                ElseIf dataRow Then
                    ba.Push lbl.text
                    colCounter = colCounter + 1
                End If
            End If
        Next j

        ' Move to the next row if a data row was captured
        If dataRow Then
            rowCounter = rowCounter + 1
            dataRow = False
        ' Mark header as captured if it's the first row
        ElseIf Not headerCaptured Then
            headerCaptured = True
            rowCounter = rowCounter + 1
        End If
    Next i

    ' Return the betterarray object
    Set get_sap_lbl_data = ba

End Function

Function get_gui_child_by_name(ByVal obj As Object, ByVal name As String) As Object

End Function

Function close_popups(sess, Optional ByRef msg As String = "") As Boolean
Dim err_wnd As ctrl_check
Dim i, j, maxtries
dbLog.log "Closing all popups"

    maxtries = 5

redo:
With sess
For i = 5 To 1 Step -1
j = 0
If j < maxtries Then
err_wnd = Exist_Ctrl(sess, i, "", True)
If err_wnd.cband Then
dbLog.log "Closing window (" & i & ")"
.FindById("wnd[" & i & "]").Close
err_wnd = Exist_Ctrl(sess, i + 1, "", True)
If err_wnd.cband Then
dbLog.log "Additional popup found.... retrying"
If Contains(err_wnd.ctext, "multiple selection") Then
' press no
If Exist_Ctrl(sess, i + 1, "/usr/btnSPOP-OPTION2", True).cband Then
.FindById("wnd[2]/usr/btnSPOP-OPTION2").press
msg = Hit_Ctrl(sess, 0, "/sbar", "Text", "Get", "")
End If
Else
GoTo redo
End If
End If
End If
ElseIf j >= maxtries Then
close_popups = False
dbLog.log "Max retries, exiting...."
Exit Function
End If
j = j + 1
Next
End With

    close_popups = True

End Function

'=============================================================================================================
' Exist_Ctrl
'
' This Function Get the SAP GUI Control Text & Type If the Control Exist
'
' Parameter : Type : Use:
' ----------------------- --------------- --------------------------------------------------------------------
' Session Object SAP Session (1...6), by Default is 1st Session
' nWnd Integer SAP Window Id (Parent=0,Child=1...n)
' Control_Id String Control Id Name
' RetMsg Boolean If True Return Control.Text
'
' Return :
' ----------------------- ------------------------------------------------------------------------------------
' ctrl_check True/False, Control.Text, Control.Type
'
'=============================================================================================================
Function Exist_Ctrl(session, nWnd, Control_Id, RetMsg) As ctrl_check
Dim retid
Dim err_chk As ctrl_check

    Set retid = session.FindById("wnd[" & nWnd & "]" & Control_Id, False)
    If Not (retid Is Nothing) Then
        err_chk.cband = True
        If RetMsg = True Then
            err_chk.ctext = session.FindById("wnd[" & nWnd & "]" & Control_Id).text
            err_chk.ctype = session.FindById("wnd[" & nWnd & "]" & Control_Id).type
        Else
            err_chk.ctext = ""
        End If
    Else
        err_chk.cband = False
        err_chk.ctext = ""
        err_chk.ctype = ""
    End If
    Exist_Ctrl = err_chk

End Function

Private Function GetHandleFromPartialCaption(ByRef lWnd As Long, ByVal sCaption As String) As Boolean

    Dim lhWndP As Long
    Dim sStr As String
    GetHandleFromPartialCaption = False
    lhWndP = FindWindow(vbNullString, vbNullString) 'PARENT WINDOW
    Do While lhWndP <> 0
        sStr = String(GetWindowTextLength(lhWndP) + 1, Chr$(0))
        GetWindowText lhWndP, sStr, Len(sStr)
        sStr = Left$(sStr, Len(sStr) - 1)
        If InStr(1, sStr, sCaption) > 0 Then
            GetHandleFromPartialCaption = True
            lWnd = lhWndP
            Exit Do
        End If
        lhWndP = GetWindow(lhWndP, GW_HWNDNEXT)
    Loop

End Function

Function get*sap_text_errors(ByVal sess, ByVal nWnd, ByVal ctrl_id, *
ByVal Count, Optional start_index As Integer = 1) As String
Dim str As String
Dim err_ctl As ctrl_check
Dim err_msg As String
Dim i

    str = ""

    ' Logging
    dbLog.log "Getting SAP errors..."
    For i = start_index To Count                 ' Not zero-indexed
        ' err_ctl = Exist_Ctrl(Sess, 0, "/usr/txtV_ERROR" & i, True)
        If Contains(ctrl_id, "[") Then
            err_ctl = Exist_Ctrl(sess, nWnd, ctrl_id & i & ",0]", True)
            If err_ctl.cband Then
                err_msg = Hit_Ctrl(sess, nWnd, ctrl_id & i & ",0]", "Text", "Get", "")
                Debug.Print err_msg
                str = str & vbCrLf & " " & err_msg
            End If
        ElseIf Contains(ctrl_id, "txtMESSTXT") Then
            ' replace right numeric if exists
            If IsNumeric(Right(ctrl_id, 1)) Then ctrl_id = Replace(ctrl_id, Right(ctrl_id, 1), "")
            err_ctl = Exist_Ctrl(sess, nWnd, ctrl_id & i, True)
            If err_ctl.cband Then
                err_msg = Hit_Ctrl(sess, nWnd, ctrl_id & i, "Text", "Get", "")
                Debug.Print err_msg
                str = str & vbCrLf & " " & err_msg
            End If
        Else
            err_ctl = Exist_Ctrl(sess, nWnd, ctrl_id & i, True)
            If err_ctl.cband Then
                err_msg = Hit_Ctrl(sess, nWnd, ctrl_id & i, "Text", "Get", "")
                Debug.Print err_msg
                str = str & vbCrLf & " " & err_msg
            End If
        End If
    Next

    dbLog.log "str contents are: (" & str & ")"

    get_sap_text_errors = str

End Function

'=============================================================================================================
' Get_Status
'
' This Function Get the Text of SAP GUI Status Bar Control (All,Right,Left)
'
' Parameter : Type : Use:
' ----------------------- --------------- --------------------------------------------------------------------
' Session Object SAP Session (1...6), by Default is 1st Session
' nWnd Integer SAP Window Id (Parent=0,Child=1...n)
' nChar String Number of Characters (Right,Left)
' nDir String A=All Text,R=Right Side,L=Left Side
'
' Return :
' ----------------------- ------------------------------------------------------------------------------------
' SAP GUI Status Bar Text
'
'=============================================================================================================
Function Get_Status(session, nWnd, nChar, nDir) As String
Dim err_ctl As ctrl_check
Dim ctrl_id, aux_str, ret_str As String

    ctrl_id = "/sbar"                            'Status Bar Control
    err_ctl = Exist_Ctrl(session, nWnd, ctrl_id, True)
    If err_ctl.cband = True Then
        aux_str = Hit_Ctrl(session, nWnd, ctrl_id, "Text", "Get", "")
        If Len(Trim(aux_str)) > 0 Then
            Select Case (nDir)
            Case "A":                            'All Characters.
                ret_str = Trim(aux_str)
            Case "R":                            'nChar at Right Side.
                ret_str = Right(Trim(aux_str), nChar)
            Case "L":                            'nChar at Left Side.
                ret_str = Left(Trim(aux_str), nChar)
            End Select
        Else
            ret_str = ""
        End If
    End If
    Get_Status = ret_str

End Function

Function get*tree_item(ByVal objSess As Variant, ByVal table_id As String, *
Optional ByVal idxRow As Integer = 0, Optional ByVal idxCol As Integer = 0) As Variant
Dim rVal, tree, row, col

    With objSess
        Set tree = .FindById(table_id)
        rVal = tree.getItemText("          " & idxRow, "C        " & idxCol)
        .FindById("wnd[0]").SendVKey 3           ' F3 back
    End With

    get_tree_item = rVal

End Function

'=============================================================================================================
' Hit_Ctrl
'
' This Function Hit a SAP GUI Control According to Identifier,Event & value
'
' Parameter : Type : Use:
' ----------------------- --------------- --------------------------------------------------------------------
' Session Object SAP Session (1...6), by Default is 1st Session
' nWnd Integer SAP Window Id (Parent=0,Child=1...n)
' Control_Id String Control Id Name
' Event_Id String SAP Control Type of Event
' Event_Id_Opt String SAP Control Action
' Event_Id_Value String SAP Control Value
'
' Return :
' ----------------------- ------------------------------------------------------------------------------------
' SAP GUI Control Value/Text
'
'=============================================================================================================
Function Hit_Ctrl(session, nWnd, Control_Id, Event_Id, Event_Id_Opt, Event_Id_Value) As String
Dim aux_str As String
aux_str = ""

    On Error GoTo errlv

    Select Case (Event_Id)
    Case "Maximize":
        session.FindById("wnd[" & nWnd & "]" & Control_Id).maximize
    Case "Minimize":
        session.FindById("wnd[" & nWnd & "]" & Control_Id).minimize
    Case "Press":
        session.FindById("wnd[" & nWnd & "]" & Control_Id).press
    Case "Select":
        session.FindById("wnd[" & nWnd & "]" & Control_Id).Select
    Case "Selected":
        Select Case (Event_Id_Opt)
        Case "Get":
            aux_str = session.FindById("wnd[" & nWnd & "]" & Control_Id).Selected
        Case "Set":
            Select Case (Event_Id_Value)
            Case "True":
                session.FindById("wnd[" & nWnd & "]" & Control_Id).Selected = True
            Case "False":
                session.FindById("wnd[" & nWnd & "]" & Control_Id).Selected = False
            End Select
        End Select
    Case "Focus":
        session.FindById("wnd[" & nWnd & "]" & Control_Id).SetFocus
    Case "Text":
        Select Case (Event_Id_Opt)
        Case "Get":
            aux_str = session.FindById("wnd[" & nWnd & "]" & Control_Id).text
        Case "Set":
            session.FindById("wnd[" & nWnd & "]" & Control_Id).text = Event_Id_Value
        End Select
    Case "Position":
        Select Case (Left(Event_Id_Opt, 3))
        Case "Get":
            Select Case (Right(Event_Id_Opt, 1))
            Case "V":
                aux_str = CStr(session.FindById("wnd[" & nWnd & "]" & Control_Id).verticalScrollbar.Position)
            Case "H":
                aux_str = CStr(session.FindById("wnd[" & nWnd & "]" & Control_Id).horizontalScrollbar.Position)
            End Select
        Case "Set":
            Select Case (Right(Event_Id_Opt, 1))
            Case "V":
                session.FindById("wnd[" & nWnd & "]" & Control_Id).verticalScrollbar.Position = CInt(Event_Id_Value)
            Case "H":
                session.FindById("wnd[" & nWnd & "]" & Control_Id).horizontalScrollbar.Position = CInt(Event_Id_Value)
            End Select
        End Select
    Case "ContextButton":
        session.FindById("wnd[" & nWnd & "]" & Control_Id).pressToolbarContextButton Event_Id_Value
    Case "ContextMenu":
        session.FindById("wnd[" & nWnd & "]" & Control_Id).selectContextMenuItem Event_Id_Value
    Case "Shell":
        Select Case (Event_Id_Opt)
        Case "CurrentCellRow":
            session.FindById("wnd[" & nWnd & "]" & Control_Id).CurrentCellRow = CInt(Event_Id_Value)
        Case "ClickCurrentCell":
            session.FindById("wnd[" & nWnd & "]" & Control_Id).singleClickCurrentCell
        Case "SelectedRows":
            session.FindById("wnd[" & nWnd & "]" & Control_Id).SelectedRows = Event_Id_Value
        Case "DoubleClickCurrentCell":
            session.FindById("wnd[" & nWnd & "]" & Control_Id).DoubleClickCurrentCell
        End Select
    Case "GetAbsoluteRow":
        Select Case (Event_Id_Value)
        Case "True":
            session.FindById("wnd[" & nWnd & "]" & Control_Id).getAbsoluteRow(CInt(Event_Id_Opt)).Selected = True
        Case "False":
            session.FindById("wnd[" & nWnd & "]" & Control_Id).getAbsoluteRow(CInt(Event_Id_Opt)).Selected = False
        End Select
    Case "GetCellValue"                          ' .findById(Objname).GetCellValue(i, "SELTEXT")
        Select Case (Event_Id_Opt)
        Case "SELTEXT"
            aux_str = session.FindById("wnd[" & nWnd & "]" & Control_Id).GetCellValue(CInt(Event_Id_Value), "SELTEXT")
        Case "VARIANT"
            aux_str = session.FindById("wnd[" & nWnd & "]" & Control_Id).GetCellValue(CInt(Event_Id_Value), "VARIANT")
        Case "TEXT"
            aux_str = session.FindById("wnd[" & nWnd & "]" & Control_Id).GetCellValue(CInt(Event_Id_Value), "TEXT")
        End Select
    End Select

    Hit_Ctrl = aux_str

errlv:

End Function

Function layout_popup(sess, tCode) As Boolean
On Error GoTo errlv
With sess
Select Case LCase(tCode)
Case "lx03", "lx02"
.FindById("wnd[0]/tbar[1]/btn[33]").press ' Select Layout
Case "vt11"
.FindById("wnd[0]/mbar/menu[3]/menu[0]/menu[1]").Select ' Choose Layout Button
Case "zmdesnr"
If Exist_Ctrl(sess, 0, "/tbar[1]/btn[33]", True).cband Then
.FindById("wnd[0]/tbar[1]/btn[33]").press
End If
Case Else
End Select
End With
layout_popup = True
Exit Function
errlv:

End Function

'=============================================================================================================
' Log_In_SAP
'
' This Create o SigOn SAP System.
'
' Parameter : Type : Use:
' ----------------------- --------------- --------------------------------------------------------------------
' Session Object SAP Session (1...6), by Default is 1st Session
' nWnd Integer SAP Window Id (Parent=0,Child=1...n)
' nChar String Number of Characters (Right,Left)
' nDir String A=All Text,R=Right Side,L=Left Side
'
' Return :
' ----------------------- ------------------------------------------------------------------------------------
' SAP GUI Object
'
'=============================================================================================================
Function Log_In_SAP(Optional n) As Variant
Dim GuiAuto
Dim AppGui
Dim connection
Dim conn
Dim Session_ret
Dim session
Dim sess
Dim authPath
Dim authFile
Dim appPath
Dim appName

    Dim Whwnd As Long
    Dim ask As Boolean
    Dim creds As params_struct
    Dim err_chk As error_check
    Dim err_ctl As ctrl_check
    Dim err_msg As String

    Dim clientID, UserId, Password, language As String
    Dim instanceID, s, vN
    On Error GoTo errlv

    s = get_var(wsOut, "SapConnectionName", 0, 1)
    ' s = "02. PN1 Production System - DT"

    instanceID = Right(s, Len(s) - InStrRev(s, " "))

    authFile = "cryptauth_" & instanceID & ".txt"
    authPath = Environ$("UserProfile") & "\Documents\SAP\"

    ' Check SAP Executable Path exists, if not re-assign
    Dim FSO As New FileSystemObject
    For Each vN In Array("C:\Program Files\SAP\FrontEnd\SAPgui\saplogon.exe", _
                         "C:\Program Files (x86)\SAP\FrontEnd\SAPgui\saplogon.exe")
        appPath = vN
        If FSO.FileExists(appPath) Then Exit For
    Next

    appName = s
    'appName = "Prod Instance"
    'appName = "Dev Instance"

    Application.DisplayAlerts = False

Top:
Dim lhWndP As Long
If GetHandleFromPartialCaption(lhWndP, "SAP Logon") Then
Whwnd = lhWndP
' If IsWindowVisible(lhWndP) = True Then
' End If
End If
If Whwnd = 0 Then
dbLog.log "SAP Window not found, starting SAP Application..."
'If Window Not Found Then Execute SAPLogon Application
'l = ShellExecute(Application.hwnd, "Open", "C:\Program Files\SAP\FrontEnd\SAPgui\saplogon.exe", "", "", 1)
Call Shell(appPath, vbNormalNoFocus)
DoEvents
Time_Event 50000 'Wait some Seconds
Else
'If SAPLogon is Found then Hide Window, 04-14-09
' Whwnd = SetWindowPos(Whwnd, 0, 0, 0, 0, 0, SWP_HIDEWINDOW)
Set GuiAuto = GetObject("SAPGUI") 'SAPLogon registers object SapGuiAuto in the Running Object Table(ROT) as SAPGUI
End If
'Verify If GuiAuto Object Exist
If IsEmpty(GuiAuto) Then
Set GuiAuto = GetObject("SAPGUI")
End If
'Verify If GuiAuto Object Exist and Set to Scripting Engine
If IsObject(GuiAuto) Then
Set AppGui = GuiAuto.GetScriptingEngine '"SAPGUI returns the interface to the Scripting Components highest level using "GetScriptingEngine"
End If
If Not AppGui Is Nothing Then
'Trace Connections available of current SAP Application
For Each connection In AppGui.Children
'If conn.DisableByServer = False Then
'End If
Set conn = connection
Next
If IsEmpty(conn) Then
dbLog.log "SAPConn is empty, starting login." & vbCrLf & "Using Conn (" & appName & ")"
'If there is not Connection Available...open a New Connection...
'By Default {InstanceName} is Opened

            ' Evaluate (IsObject(AppGui.OpenConnection(appName, False, True)))
            Set conn = AppGui.OpenConnection(appName, False, True)

            Set GuiAuto = GetObject("SAPGUI")
            Set AppGui = GuiAuto
            Set AppGui = GuiAuto.GetScriptingEngine
            Set conn = AppGui.Children(0)
        End If
        'Trace Sessions available of current Connection
        For Each session In conn.Children
            'If conn.DisableByServer = False Then
            'End If
            Set sess = session
        Next
        Set sess = conn.Children(0)

        ' Check if already logged in
        If Not Contains(sess.Info.Transaction, "S000") Then GoTo loggedin

        'Get Parameters for Logon in to SAP
        'Get info for Client,UserId,Password,Language.
        clientID = "025"
        UserId = Get_Text(authPath & authFile, 1)
        Password = Get_Text(authPath & authFile, 2)
        ' If authfile not found, use popup
        If Len(UserId) = 0 Then
            ask = True
            UserId = InputBox("User Id : ", "SAP Logon...", NetUserName())
        End If
        If Len(Password) = 0 Then
            ask = True
            Password = InputBoxPassword("Enter password", "PassWord : ")
        End If
        ' Ask if user would like to save Credentials
        If ask = True Then
            If MsgBox("Would you like to save your username/password?", vbYesNo) = vbYes Then
                Call Save_Credentials(authPath, authFile, UserId, Password)
            End If
        End If
        language = "EN"
        'Set to Parameters Structure
        creds.clientID = clientID
        creds.user = UserId
        creds.pass = Password
        creds.language = language
        'Check if Logon was Successfully
        Dim msg
        err_chk = Check_wnd(sess, 0, creds)
        ' possible multi logon window popup
        err_ctl = Exist_Ctrl(sess, 1, "", True)
        If err_ctl.cband Then
            Debug.Print err_ctl.ctext
            If Contains(err_ctl.ctext, "multiple logon") Then
                sess.FindById("wnd[1]/usr/radMULTI_LOGON_OPT1").Select
                sess.FindById("wnd[1]/usr/radMULTI_LOGON_OPT1").SetFocus
                sess.FindById("wnd[1]").SendVKey 0
            End If
            Debug.Print err_ctl.ctype
        End If
        err_msg = Hit_Ctrl(sess, 0, "/sbar", "Text", "Get", "")
        If Contains(err_msg, "incorrect", 0) Then
            msg = "Statusbar: " & err_msg & ". Please clear credentials and try again."
            dbLog.log msg
            If msgSwitch Then MsgBox msg
            Exit Function
        ElseIf Contains(err_msg, "new password", 0) Then
            msg = "Statusbar: " & err_msg & ". Please Update your password."
            dbLog.log msg
            If msgSwitch Then MsgBox msg
            Exit Function
        End If
        If err_chk.bchgb = True Then
            dbLog.log "SAP Login successfull - " & Format(Now, time_form)
            Set GuiAuto = GetObject("SAPGUI")
            Set AppGui = GuiAuto
            Set AppGui = GuiAuto.GetScriptingEngine
            Set conn = AppGui.Children(0)
            Set session = conn.Children(0)
        End If
        ''' Window 1
        ''err_chk = Check_wnd(Session, 1, In_Params)
        ''If err_chk.bchgb = True Then
        ''   Set Session_ret = Conn.Children(0)
        ''End If
        ' Close popup if exists Updated to close popup after login
        ' regardless of title -- 181203
        err_ctl = Exist_Ctrl(sess, 1, "", True)
        If err_ctl.cband Then
            sess.FindById("wnd[1]").Close
        End If
    Else
        dbLog.log "Conn exists"
        Set conn = AppGui.Connections(0)
        'Set conn = AppGui.Children(0)
    End If

loggedin:
If Not conn Is Nothing Then
'Trace Sessions available of current Connection
For Each session In conn.Children
'If conn.DisableByServer = False Then
'End If
Set sess = session
Debug.Print "Current tCode is " & sess.Info.Transaction
' Close popup if exists
err_ctl = Exist_Ctrl(sess, 1, "", True)
If err_ctl.cband Then
sess.FindById("wnd[1]").Close
End If
' Check if on login screen
If Contains(sess.Info.Transaction, "S000", 0) Then
dbLog.log "Conn exists but is empty. Reopening app."
Set AppGui = Nothing
Set conn = Nothing
Set GuiAuto = Nothing
Call Close_Conn
GoTo Top
End If
'**\*\***\*\*\*\***\*\***\*\*\*\***\*\***\*\*\*\***\*\***
'Conn.CloseSession(Sess.Name)
'Sess.Children.Count
'**\*\***\*\*\*\***\*\***\*\*\*\***\*\***\*\*\*\***\*\***
Next
If IsEmpty(sess) Then
Else
'Note: Show SAP Easy Access Window, 04-14-09
Whwnd = FindWindow(vbNullString, "SAP Easy Access")
'Whwnd = SetWindowPos(Whwnd, 0, 0, 0, 0, 0, SWP_SHOWWINDOW)
Set Session_ret = conn.Children(0)
End If
End If
'System Message Ignored...=1(Enabled) / 0(Disable)
Session_ret.TestToolMode = 1
'Check if Waiting Data From the Server....and Wait if happend
If Session_ret.busy = True Then
End If
Set Log_In_SAP = Session_ret
Exit Function

errlv:
dbLog.log err.description: Resume Next
End Function

'=============================================================================================================
'
' NetUserName
'
' This function Run MD07 Modules and Menu_Update_Template.
'
' Parameter : Use:
' ----------------------- ----------------------------------------------------------------------------------
' Nothing Nothing
'
' Return :
' -----------------------
' Nothing
'
'=============================================================================================================
Public Function NetUserName() As String
Dim i As Long
Dim userName As String \* 255
i = WNetGetUser("", userName, 255)
If i = 0 Then
NetUserName = Left$(userName, InStr(userName, Chr$(0)) - 1)
Else
NetUserName = ""
End If
End Function

Function sap*filter_sort(ByVal sess As Variant, nWnd As Integer, ByVal tCode As String, *
ByVal list As BetterArray, Optional ByVal limit As Integer = 100) As Variant

    ' should run this after layout is setup to get column positions for filter
    Dim rVal As New BetterArray, msg As String
    With sess
        Select Case LCase(tCode)
        Case "lx03"
            '   get column headers
            Set rVal = get_info_lbl(sess, nWnd, "/usr/lbl", 1, 5, 200, 6, 1)
            Debug.Print list
            ' idx = rVal.IndexOf()
            ' err_ctl = Hit_Ctrl(sess, nWnd, "/usr/lbl")

        Case Else
            msg = "sap_filter_sort not set up yet"
        End Select

    End With

    Exit Function

errlv:
sap_filter_sort = msg
End Function

Function SelectLayout(session, nWnd, ObjectName, layoutName) As Boolean
Dim LayoutNames As New Collection
Dim err_wnd As ctrl_check
Dim i, j, r, name, obj, row_count

    With session

        Time_Event

        dbLog.log "Checking if layout select window is present."
        err_wnd = Exist_Ctrl(session, nWnd, "", True)
        If err_wnd.cband Then
            dbLog.log "Window with title (" & err_wnd.ctext & ") found."
        Else
            dbLog.log "Window not open, exiting..."
            GoTo errlv
        End If

        dbLog.log "Checking if layout exists..."
        ' Set Object updated 181203
        err_wnd = Exist_Ctrl(session, nWnd, ObjectName, True)
        If Not err_wnd.cband Then GoTo errlv
        Set obj = .FindById("wnd[" & nWnd & "]" & ObjectName)

        ' Select all columns
        '.findById(ObjectName).SelectAll

        row_count = obj.rowcount
        Debug.Print "Obj has " & row_count & " rows"
        ' Scroll down to end (in case long)
        If row_count > 0 Then                    ' // added in case no layouts exist
            obj.firstvisiblerow = (row_count - 1)
            r = obj.firstvisiblerow
            Debug.Print "Scrolldown - First visible row = " & r
            Debug.Print obj.type
            Debug.Print obj.text
            Debug.Print obj.name
            'obj.setcurrentcell -1, "TEXT"
        End If

        ' Add names to collection(LayoutNames)
        Set LayoutNames = Nothing
        For i = 0 To row_count - 1
            name = obj.GetCellValue(i, "VARIANT")
            If Not IsError(name) Then
                ' Debug.Print obj.GetCellValue(i, "VARIANT") & " (" & _
                obj.GetCellValue(i, "TEXT") & ")"
                LayoutNames.add UCase(name)
            End If
        Next

        Debug.Print "Found " & LayoutNames.Count & " Layouts"

        If IsInArray(UCase(layoutName), LayoutNames) Then
            j = IndexInArray(UCase(layoutName), LayoutNames)
            obj.SetCurrentCell j, "VARIANT"
            obj.SelectedRows = j
            '.findById _
            ("wnd[1]/tbar[0]/btn[2]").press
            obj.DoubleClickCurrentCell
            dbLog.log "Selected."
        Else
            dbLog.log "Layout (" & layoutName & ") not found. " & Format(Now, time_form)
            Exit Function
        End If

        ' Unset Object
        Set obj = Nothing


    End With

    SelectLayout = True
    Exit Function

errlv:

End Function

Function SetupLayout(session, nWnd, baseObjName, layoutName, list As Collection, limit, \_
Optional ByVal bNoSave As Boolean = False) As Boolean
Dim listRows As New Collection
Dim objName, ObjListLeft, ObjListRight, ObjButtonToLeft, ObjButtonToRight
Dim i, j, name, GridLeft, GridRight
Dim aux_str As String
Dim err_ctrl As ctrl_check
aux_str = ""

    Debug.Print list.Count

    objName = "wnd[" & nWnd & "]" & baseObjName
    ObjListLeft = objName & "/cntlCONTAINER2_LAYO/shellcont/shell"
    ObjListRight = objName & "/cntlCONTAINER1_LAYO/shellcont/shell"
    ObjButtonToLeft = objName & "/btnAPP_WL_SING"
    ObjButtonToRight = objName & "/btnAPP_FL_SING"


    Debug.Print baseObjName
    '/usr/tabsG_TS_ALV/tabpALV_M_R1/ssubSUB_DYN0510:SAPLSKBH:0620
    Debug.Print baseObjName & "/cntlCONTAINER2_LAYO/shellcont/shell"
    '/usr/tabsG_TS_ALV/tabpALV_M_R1/ssubSUB_DYN0510:SAPLSKBH:0620/cntlCONTAINER2_LAYO/shellcont/shell
    Debug.Print ObjListLeft
    'Session.findById("wnd[1]/usr/tabsG_TS_ALV/tabpALV_M_R1/ssubSUB_DYN0510:SAPLSKBH:0620/cntlCONTAINER2_LAYO/shellcont/shell").currentCellRow = 4
    With session

        ' Assign objects to var
        Set GridLeft = .FindById(ObjListLeft)
        Set GridRight = .FindById(ObjListRight)

        GridLeft.CurrentCellRow = 0

        ' Move current items from left grid to right grid

        GridLeft.SelectedRows = "0-" & (GridLeft.rowcount - 1)
        .FindById(ObjButtonToRight).press


        .FindById(ObjListLeft).CurrentCellRow = -1
        GridRight.CurrentCellRow = 1
        GridRight.selectColumn "SELTEXT"
        GridRight.pressColumnHeader "SELTEXT"

        On Error GoTo endloop2
        name = "*"

        For Each j In list
            i = 0
            Do While i < limit
                name = Hit_Ctrl(session, nWnd, baseObjName & "/cntlCONTAINER1_LAYO/shellcont/shell", "GetCellValue", "SELTEXT", i)
                Debug.Print name
                If UCase(CStr(j)) = UCase(name) Then
                    GridRight.CurrentCellRow = (i)
                    GridRight.DoubleClickCurrentCell
                    AppWait (1)
                    Exit Do
                Else: i = i + 1
                End If
            Loop
        Next

        'i = 0
        'Do While Len(Name) > 0
        'Name = Hit_Ctrl(Session, nWnd, BaseObjName & "/cntlCONTAINER1_LAYO/shellcont/shell", "GetCellValue", "SELTEXT", i)
        'Debug.Print Name
        '    If IsInArray(UCase(Name), List) Then
        '        .findById(ObjListRight).currentcellrow = (i)
        '        .findById(ObjListRight).DoubleClickCurrentCell
        '        i = 0
        '    Else: i = i + 1
        '    End If
        'Loop

endloop2:
err.Clear
dbLog.log "Added " & list.Count & " items to Layout"
If bNoSave Then GoTo no*save
' Save layout button
.FindById("wnd[1]/tbar[0]/btn[5]").press
' User-Specific
.FindById *
("wnd[2]/usr/tabsG50*TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/chkG51_USPEC") *
.Selected = True
' Default Layout Yes/No
.FindById _
("wnd[2]/usr/tabsG50_TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/chkLTVARIANT-DEFAULTVAR") _
.Selected = False
' Save as name
.FindById _
("wnd[2]/usr/tabsG50_TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/txtLTDX-VARIANT") _
.text = layoutName
' Save as Description
.FindById("wnd[2]/usr/tabsG50*TABSTRIP/tabpTAB_D0501/ssubD0505_SUBSCREEN:SAPLSLVC_DIALOG:0501/txtLTDXT-TEXT") *
.text = layoutName

        ' Green Checkmark wnd2
        .FindById("wnd[2]/tbar[0]/btn[0]").press
        ' LayoutExists Overwrite Y/N
        err_ctrl = Exist_Ctrl(session, 3, "", True)
        If err_ctrl.cband Then
            '.findById("wnd[3]/usr/btnSPOP-OPTION2").press ' No
            .FindById("wnd[3]/usr/btnSPOP-OPTION1").press ' Yes
        End If

no_save:
' Green Checkmark wnd1
.FindById("wnd[1]/tbar[0]/btn[0]").press
aux_str = Hit_Ctrl(session, 0, "/sbar", "Text", "Get", "")
dbLog.log str_form & "Layout (" & layoutName & ") Successfully Setup: " & Format(Now, time_form) & str_form
End With

    SetupLayout = True

End Function

Function SetupLayout*li(session, tCode, nWnd, baseObjName, layoutName, list As Collection, limit, *
Optional ByVal bNoSave As Boolean = False) As Boolean
Dim listRows As New Collection
Dim wnd, objName, ObjListLeft, ObjListRight, ObjButtonToLeft, ObjButtonToRight
Dim nwObjListLeft, nwObjListRight
Dim i, j, name_missing, name, GridLeft, GridRight
Dim rc, vrc, scrl_pos, scrl_pos2, counter
Dim aux_str As String
Dim err_ctrl As ctrl_check
aux_str = ""

    Debug.Print list.Count

    wnd = "wnd[" & nWnd & "]"
    objName = baseObjName
    ObjListLeft = wnd & objName & "/tblSAPLSKBHTC_WRITE_LIST"
    ObjListRight = wnd & "/usr/tblSAPLSKBHTC_FIELD_LIST"
    ObjButtonToLeft = wnd & "/usr/btnAPP_WL_SING"
    ObjButtonToRight = wnd & "/usr/btnAPP_FL_SING"

    nwObjListLeft = objName & "/tblSAPLSKBHTC_WRITE_LIST"
    nwObjListRight = "/usr/tblSAPLSKBHTC_FIELD_LIST"


    Debug.Print baseObjName
    'wnd[1]/usr/tabsTS_LINES/tabpLI01/ssubSUB810:SAPLSKBH:0810
    Debug.Print ObjListLeft
    'wnd[1]/usr/tabsTS_LINES/tabpLI01/ssubSUB810:SAPLSKBH:0810/tblSAPLSKBHTC_WRITE_LIST
    Debug.Print nwObjListLeft

    Debug.Print ObjListRight
    'wnd[1]/usr/tblSAPLSKBHTC_FIELD_LIST
    With session

        ' Get dims for left obj
        rc = Int(.FindById(ObjListLeft).rowcount)
        vrc = Int(.FindById(ObjListLeft).visiblerowcount)
        scrl_pos = .FindById(ObjListLeft).verticalScrollbar.Position

        Debug.Print "Row count is " & rc
        Debug.Print "Visible rowcount is " & vrc
        Debug.Print "Vertical Scrollbar Position is " & scrl_pos

clrLeft:
i = 1: j = 1
dbLog.log rc & " items found in current layout. Clearing..."
For i = i To rc
err_ctrl = Exist_Ctrl(session, 1, nwObjListLeft & "/txtGT_WRITE_LIST-SELTEXT[0," & j & "]", True)
If err_ctrl.cband Then
If Not Contains(err_ctrl.ctext, "**\_**", 0) Then
Debug.Print .FindById(ObjListLeft).Rows(j).item(0).displayedtext
.FindById(ObjListLeft).Rows(j).Selected = True
' Move current items from left grid to right grid
j = j + 1
If j = 12 Then
err_ctrl = Exist_Ctrl(session, 1, nwObjListLeft, True)
If err_ctrl.cband = True Then
scrl_pos = .FindById(ObjListLeft).verticalScrollbar.Position
Debug.Print "Vertical Scrollbar Position is " & scrl_pos
' Scroll Down scrl_pos + 12 (one page)
aux_str = Hit_Ctrl(session, 1, nwObjListLeft, "Position", "SetV", (scrl_pos + 12))
j = 0
End If
End If
Else
Exit For
End If
End If
Next
i = 0: j = 0
' Move to right (clear)
.FindById(ObjButtonToRight).press

        ' Start working with right list
        ' Order alphabetical
        .FindById("wnd[1]/usr/btn%#AUTOTEXT002").press

        ' Get dims for left obj
        rc = Int(.FindById(ObjListRight).rowcount)
        vrc = Int(.FindById(ObjListRight).visiblerowcount)
        scrl_pos = .FindById(ObjListRight).verticalScrollbar.Position


        On Error GoTo endloop2
        name = "*"

        For Each j In list
            If UCase(j) <> "SHIPMENT NUMBER" And UCase(j) <> "DELIVERY" And Not Contains(UCase(j), "_LAYOUT", False) Then
                counter = 1
                Do While counter < limit
                    counter = counter + 1
                    For i = 0 To 11
                        ' If rows(i).count is 0 item is blank
                        If .FindById(ObjListRight).Rows(i).Count <> 0 Then
                            name = Trim(.FindById(ObjListRight).Rows(i).item(0).displayedtext)
                        Else
                            GoTo notFound
                        End If
                        Debug.Print name
                        If UCase(CStr(j)) = UCase(name) Then
                            ' Found, select row
                            .FindById(ObjListRight).Rows(i).Selected = True
                            dbLog.log "Item (" & name & ") selected from right"
                            AppWait (1)
                            ' Move to left
                            .FindById(ObjButtonToLeft).press
                            dbLog.log "Item (" & name & ") moved to left"
                            ' Reset scrollbar to top
                            aux_str = Hit_Ctrl(session, 1, nwObjListRight, "Position", "SetV", 0)
                            ' Skip to next item in List
                            GoTo found
                        End If
                    Next
                    ' Scroll down
                    err_ctrl = Exist_Ctrl(session, 1, nwObjListLeft, True)
                    If err_ctrl.cband = True Then
                        scrl_pos = .FindById(ObjListRight).verticalScrollbar.Position
                        vrc = Int(.FindById(ObjListLeft).visiblerowcount)
                        ' Scroll Down scrl_pos + 12 (one page)
                        aux_str = Hit_Ctrl(session, 1, nwObjListRight, "Position", "SetV", (scrl_pos + 12))
                        scrl_pos2 = .FindById(ObjListRight).verticalScrollbar.Position
                        ' Check if scrollbar moved (if not it reached bottom)
                        If scrl_pos = scrl_pos2 Then

notFound:
dbLog.log "Item (" & j & ") not found, skipping"
' Reset scrollbar to top
aux_str = Hit_Ctrl(session, 1, nwObjListRight, "Position", "SetV", 0)
' Skip to next item in List
GoTo found
End If
End If
Loop
End If
found:
Next

endloop2:
err.Clear
dbLog.log "Added " & list.Count & " items to Layout"

        Select Case UCase(tCode)
        Case "MB52"
            ' Enter
            .FindById("wnd[1]/tbar[0]/btn[0]").press
            .FindById("wnd[0]/tbar[1]/btn[34]").press   ' save
            ' 12 char limit warn, but continue
            If Len(layoutName) > 12 Then
                dbLog.log "Layout name is more than 12 characters, unable to save.", _
                           msgPopup:=True, msgType:=vbInformation
                err_ctrl = Exist_Ctrl(session, 1, "", True)
                If err_ctrl.cband Then .FindById("wnd[1]").Close
            Else
                .FindById("wnd[1]/usr/ctxtLTDX-VARIANT").text = layoutName
                .FindById("wnd[1]/usr/txtLTDXT-TEXT").text = layoutName
                ' enter
                .FindById("wnd[1]").SendVKey 0
                ' LayoutExists Overwrite Y/N
                err_ctrl = Exist_Ctrl(session, 2, "", True)
                If err_ctrl.cband Then
                    '.findById("wnd[1]").Close   ' Close
                    .FindById("wnd[2]").SendVKey 0   ' Yes
                End If
            End If
        Case "LX03", "LX02"
            ' Enter
            .FindById("wnd[1]/tbar[0]/btn[0]").press
            ' Save button
            .FindById("wnd[0]/tbar[1]/btn[36]").press
            ' 12 char limit warn, but continue
            If Len(layoutName) > 12 Then
                dbLog.log "Layout name is more than 12 characters, unable to save.", _
                           msgPopup:=True, msgType:=vbInformation
                err_ctrl = Exist_Ctrl(session, 1, "", True)
                If err_ctrl.cband Then .FindById("wnd[1]").Close
            Else
                .FindById("wnd[1]/usr/ctxtLTDX-VARIANT").text = layoutName
                .FindById("wnd[1]/usr/txtLTDXT-TEXT").text = layoutName
                ' enter
                .FindById("wnd[1]").SendVKey 0
                ' LayoutExists Overwrite Y/N
                err_ctrl = Exist_Ctrl(session, 2, "", True)
                If err_ctrl.cband Then
                    '.findById("wnd[1]").Close   ' Close
                    .FindById("wnd[2]").SendVKey 0   ' Yes
                End If
            End If


        Case "LT23"
            ' Enter (Close window)
            .FindById("wnd[1]").SendVKey 0       ' Enter

            ' Save layout button
            .FindById("wnd[0]/mbar/menu[3]/menu[2]/menu[3]").Select

            ' User Specific
            .FindById("wnd[1]/usr/chkG_FOR_USER").Selected = True

            ' LayoutName
            .FindById("wnd[1]/usr/ctxtLTDX-VARIANT").text = layoutName

            ' Layout Description
            .FindById("wnd[1]/usr/txtLTDXT-TEXT").text = layoutName

            ' Enter (Save)
            .FindById("wnd[1]").SendVKey 0       ' Yes

            ' LayoutExists Overwrite Y/N
            err_ctrl = Exist_Ctrl(session, 2, "", True)
            If err_ctrl.cband Then
                '.findById("wnd[1]").Close   ' Close
                .FindById("wnd[2]").SendVKey 0   ' Yes
            End If
        Case "VT11"
            ' Enter (Close window)
            .FindById("wnd[1]").SendVKey 0       ' Enter

            ' Save layout button
            .FindById("wnd[0]/mbar/menu[3]/menu[0]/menu[3]").Select ' VT11

            ' User Specific
            .FindById("wnd[1]/usr/chkG_FOR_USER").Selected = True

            ' Save as name
            .FindById("wnd[1]/usr/ctxtLTDX-VARIANT").text = layoutName

            ' Save as Description
            .FindById("wnd[1]/usr/txtLTDXT-TEXT").text = layoutName

            ' Enter (Save)
            .FindById("wnd[1]").SendVKey 0       ' Yes

            ' LayoutExists Overwrite Y/N
            err_ctrl = Exist_Ctrl(session, 2, "", True)
            If err_ctrl.cband Then
                '.findById("wnd[1]").Close   ' Close
                .FindById("wnd[2]").SendVKey 0   ' Yes
            End If
        Case "VL06O"
            ' Enter (Close window)
            .FindById("wnd[1]").SendVKey 0       ' Enter

            ' Save Layout button
            .FindById("wnd[0]/mbar/menu[3]/menu[2]/menu[3]").Select ' VL06O

            ' User Specific
            .FindById("wnd[1]/usr/chkG_FOR_USER").Selected = True

            ' Save layout name
            .FindById("wnd[1]/usr/ctxtLTDX-VARIANT").text = layoutName

            ' Save layout description
            .FindById("wnd[1]/usr/txtLTDXT-TEXT").text = layoutName

            ' Enter (Save)
            .FindById("wnd[1]").SendVKey 0       ' Yes

            ' LayoutExists Overwrite Y/N
            err_ctrl = Exist_Ctrl(session, 2, "", True)
            If err_ctrl.cband Then
                '.findById("wnd[1]").Close   ' Close
                .FindById("wnd[2]").SendVKey 0   ' Yes
            End If
        End Select

        aux_str = Hit_Ctrl(session, 0, "/sbar", "Text", "Get", "")
        dbLog.log str_form & "Layout (" & layoutName & ")" & vbCrLf & "Sbar Msg: (" & aux_str & ")" & vbCrLf & _
                  "Successfully Setup: " & Format(Now, time_form) & str_form
    End With

    SetupLayout_li = True

End Function

Sub testcheckexport()
Dim a, sapcheck
Call Attach_SAPGUI
a = Check_Export_Window(Session1, "VT11", "DB STOCK")
End Sub

Sub testsetuplayout()
Dim sap_check As Boolean
Dim run_check As Boolean
Dim listRows As New Collection

    sap_check = Attach_SAPGUI
    Set listRows = Nothing

    Call Populate_Collection_Vertical(ThisWorkbook, "Layouts", "snr_test_layoutrows", listRows)

    Debug.Print listRows.Count

    run_check = SetupLayout_li _
                (Session1, "", 1, "/usr/tabsG_TS_ALV/tabpALV_M_R1/ssubSUB_DYN0510:SAPLSKBH:0620", "TEST_Layout", listRows, 200)

End Sub

Private Sub TestWndFind()

    Dim lhWndP As Long
    If GetHandleFromPartialCaption(lhWndP, "SAP Logon") = True Then
        If IsWindowVisible(lhWndP) = True Then
            dbLog.log "Found VISIBLE Window Handle: " & lhWndP, vbOKOnly + vbInformation, , 1, vbInformation
        Else
            dbLog.log "Found INVISIBLE Window Handle: " & lhWndP, vbOKOnly + vbInformation, , 1, vbInformation
        End If
    Else
        dbLog.log "Window 'Excel' not found!", , , 1, vbOKOnly + vbExclamation
    End If

End Sub

'=============================================================================================================
' Time_Event
'
' This Function Loop n Times
'
' Parameter : Type : Use:
' ----------------------- --------------- --------------------------------------------------------------------
' TotalTime Double Iteration(Seconds) to Lost Time.
'
' Return :
' ----------------------- ------------------------------------------------------------------------------------
'
'
'=============================================================================================================
Function Time_Event(Optional TotalTime = TOTAL_TIME)

    dbLog.log "Waiting TotalTime: " & TotalTime
    Dim ntime As Double
    For ntime = 1 To TotalTime
        DoEvents
    Next ntime

End Function

Public Function WeekNumberAbsolute(ByVal dt As Date) As Long
'''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''
' WeekNumberAbsolute
' This returns the week number of the date in DT based on Week 1 starting
' on January 1 of the year of DT, regardless of what day of week that
' might be.
' Formula equivalent:
' =TRUNC(((DT-DATE(YEAR(DT),1,0))+6)/7)
'''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''''
WeekNumberAbsolute = Int(((dt - DateSerial(Year(dt), 1, 0)) + 6) / 7)
End Function
