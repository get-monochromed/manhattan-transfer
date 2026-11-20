cmake_minimum_required(VERSION 3.24)
project(NearbyTransfer VERSION 0.2.1 LANGUAGES CXX)

find_package(Qt6 6.6 REQUIRED COMPONENTS Core Gui Widgets Qml Quick QuickControls2 DBus)
qt_standard_project_setup()
qt_add_executable(nearby-transfer main.cpp controller.cpp controller.h)
set_target_properties(nearby-transfer PROPERTIES CXX_STANDARD 20 CXX_STANDARD_REQUIRED YES)
target_link_libraries(nearby-transfer PRIVATE Qt6::Core Qt6::Gui Qt6::Widgets Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::DBus)
qt_add_resources(nearby-transfer interface PREFIX / FILES
    qml/Main.qml qml/SendPage.qml qml/ReceivePage.qml qml/TransfersPage.qml qml/TransferCard.qml qml/Surface.qml)
