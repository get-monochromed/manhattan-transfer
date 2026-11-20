#include "controller.h"

#include <QApplication>
#include <QCommandLineParser>
#include <QDBusConnection>
#include <QDir>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QWindow>

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("nearby-transfer"));
    app.setApplicationDisplayName(QStringLiteral("Manhattan Transfer"));
    app.setApplicationVersion(QStringLiteral("0.2.1"));
    app.setDesktopFileName(QStringLiteral("io.github.NearbyTransfer"));
    app.setWindowIcon(QIcon::fromTheme(QStringLiteral("document-share")));
    QQuickStyle::setStyle(QStringLiteral("org.kde.desktop"));

    auto bus = QDBusConnection::sessionBus();
    const QString windowService = QStringLiteral("io.github.NearbyTransfer.Window");
    const QString object = QStringLiteral("/io/github/NearbyTransfer/Window");
    QCommandLineParser arguments;
    arguments.addHelpOption();
    arguments.addVersionOption();
    const QCommandLineOption backgroundOption(QStringLiteral("background"), QObject::tr("Start in the system tray."));
    arguments.addOption(backgroundOption);
    arguments.addPositionalArgument(QStringLiteral("paths"), QObject::tr("Files and folders to share."), QStringLiteral("[paths...]"));
    arguments.process(app);
    auto paths = arguments.positionalArguments();
    const bool background = arguments.isSet(backgroundOption) && paths.isEmpty();
    for (auto &path : paths) {
        if (const auto url = QUrl(path); url.isLocalFile()) {
            path = url.toLocalFile();
        } else {
            path = QDir::current().absoluteFilePath(path);
        }
    }
    if (bus.isConnected() && !bus.registerService(windowService)) {
        if (background) {
            return 0;
        }
        auto call = QDBusMessage::createMethodCall(windowService, object, QString(), QStringLiteral("Activate"));
        call.setArguments({paths});
        auto reply = bus.call(call, QDBus::Block, 3000);
        return reply.type() == QDBusMessage::ErrorMessage ? 1 : 0;
    }

    Controller controller;
    bus.registerObject(object, &controller, QDBusConnection::ExportScriptableSlots);
    controller.queuePaths(paths);
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("transferController"), &controller);
    engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }
    auto *window = qobject_cast<QWindow *>(engine.rootObjects().constFirst());
    controller.attachWindow(window);
    if (background) {
        app.setQuitOnLastWindowClosed(false);
    } else {
        window->show();
    }
    return app.exec();
}
