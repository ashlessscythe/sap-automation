If Not IsObject(application) Then
Set SapGuiAuto = GetObject("SAPGUI")
Set application = SapGuiAuto.GetScriptingEngine
End If
If Not IsObject(connection) Then
Set connection = application.Children(0)
End If
If Not IsObject(session) Then
Set session = connection.Children(0)
End If
If IsObject(WScript) Then
WScript.ConnectObject session, "on"
WScript.ConnectObject application, "on"
End If
session.findById("wnd[0]").resizeWorkingPane 139,27,false
session.findById("wnd[0]/usr/cntlIMAGE_CONTAINER/shellcont/shell/shellcont[0]/shell").doubleClickNode "F00002"
session.findById("wnd[0]").sendVKey 17
session.findById("wnd[1]").sendVKey 8
session.findById("wnd[1]/usr/cntlALV_CONTAINER_1/shellcont/shell").currentCellRow = 5
session.findById("wnd[1]/usr/cntlALV_CONTAINER_1/shellcont/shell").selectedRows = "5"
session.findById("wnd[1]/usr/cntlALV_CONTAINER_1/shellcont/shell").doubleClickCurrentCell
session.findById("wnd[0]/usr/radR_ALL").select
session.findById("wnd[0]/usr/ctxtS_MATNR-LOW").text = "35552893"
session.findById("wnd[0]/usr/txtS_SIGNI-LOW").text = "trl"
session.findById("wnd[0]/usr/ctxtS_D_DATE-LOW").text = "2025-07-25"
session.findById("wnd[0]/usr/ctxtS_D_DATE-HIGH").text = "2025-08-05"
session.findById("wnd[0]/usr/radR_ALL").setFocus
session.findById("wnd[0]").sendVKey 8
