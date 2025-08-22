session.findById("wnd[0]/tbar[0]/okcd").text = "/nvt11"
session.findById("wnd[0]").sendVKey 0
// load variant key
session.findById("wnd[0]").sendVKey 17  
// execute (close wnd[1])
session.findById("wnd[1]").sendVKey 8
session.findById("wnd[1]/usr/cntlALV*CONTAINER_1/shellcont/shell").currentCellRow = 1
session.findById("wnd[1]/usr/cntlALV_CONTAINER_1/shellcont/shell").selectedRows = "1"
session.findById("wnd[1]/usr/cntlALV_CONTAINER_1/shellcont/shell").doubleClickCurrentCell
// delivery low
session.findById("wnd[0]/usr/ctxtS_VBELN-LOW").text = "delivery"
session.findById("wnd[0]/usr/ctxtS_VBELN-LOW").setFocus
session.findById("wnd[0]/usr/ctxtS_VBELN-LOW").caretosition = 8
// multi delivery button
session.findById("wnd[0]/usr/btn%\_S_VBELN*%_APP_%-VALU*PUSH").press
// there is an abstract fn (paste_values_with_scroll) that does this
session.findById("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,1]").text = "bruh"
session.findById("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,1]").setFocus
session.findById("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,1]").caretPosition = 4
session.findById("wnd[1]").sendVKey 0
session.findById("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,2]").text = "brtuhhhh"
session.findById("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,2]").setFocus
session.findById("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,2]").caretPosition = 8
session.findById("wnd[1]").sendVKey 0
// execute close wnd[1]
session.findById("wnd[1]").sendVKey 8
//execute
session.findById("wnd[0]").sendVKey 8
session.findById("wnd[1]").sendVKey 0
session.findById("wnd[0]/usr/btn%\_S_VBELN*%_APP_%-VALU_PUSH").press
// choose layout from here we have fn that handle exporting and choosing layout
session.findById("wnd[1]").sendVKey 16
session.findById("wnd[1]/usr/tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE/ctxtRSCSEL_255-SLOW_I[1,0]").text = ""
session.findById("wnd[1]").sendVKey 24
session.findById("wnd[1]").sendVKey 8
session.findById("wnd[0]").sendVKey 8
session.findById("wnd[0]/tbar[1]/btn[32]").press
session.findById("wnd[1]").close
session.findById("wnd[0]/mbar/menu[3]/menu[0]/menu[1]").select
session.findById("wnd[1]/tbar[0]/btn[12]").press
